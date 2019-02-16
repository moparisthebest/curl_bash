# curl_bash

Be evil to curl | bash executors.

Inspired by [https://www.idontplaydarts.com/2016/04/detecting-curl-pipe-bash-server-side/](https://www.idontplaydarts.com/2016/04/detecting-curl-pipe-bash-server-side/), written more as a fun way to write something with rust than a useful tool.

Usage:

- `curl_bash`
- `curl -sSkv http://localhost:5555/setup.bash | bash`
- `curl -sSkv http://localhost:5555/setup.bash | cat`
- `curl -sSkv http://localhost:5555/setup.bash | cat -v`

License
-------
GNU/AGPLv3 - Check LICENSE.md for details
