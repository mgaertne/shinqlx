# shinqlx
![python](https://img.shields.io/badge/python-3.8%7C3.9%7C3.10%7C3.11%7C3.12-blue.svg)
![Tests](https://github.com/mgaertne/shinqlx/actions/workflows/ci.yml/badge.svg)
[![codecov](https://codecov.io/gh/mgaertne/shinqlx/branch/main/graph/badge.svg?token=VK9QI52BZX)](https://codecov.io/gh/mgaertne/shinqlx)

ShiN0's Quake Live eXtension, implemented in Rust. Most functionality from [minqlx](https://raw.githubusercontent.com/MinoMino/minqlx) should work. Support for Python 3.8 and above should work out of the box.

Some limitations apply for certain minqlx functions maybe used in plugins.
* 32-bit implementation may not work. It's untested.
* Some compatibility might not work, as this implementation is not yet fully tested.

# Compilation and installation
- Install rust with the default profile and the nightly toolchain and Rust's build tool cargo, and make sure to add the rust-src component:
```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup install nightly --profile default
rustup component add rust-src
rustup default nightly
```

- Install Python 3.

```shell
sudo apt-get update
sudo apt-get -y install python3 python3-dev python3-pip
```

- Make sure, that you have installed Python 3.9 or later:
```shell
python3 --version
```

- Now you should get Redis, Git and build utils which will be used by minqlx's plugins and for compiling shinqlx:
```shell
sudo apt-get -y install redis-server pkg-config libssl-dev git build-essential
```

- To avoid interference with operating system packages (and future adaptions from Debian v12 onwards for example), I recommend installing a python virtual environment to install the packges that shinqlx/minqlx or your plugins need:
```shell
python3 -m venv ~/qlds/.venv
source ~/qlds/.venv/bin/activate
```
If you need to install additional packages later, you can activate the virtual python environment with `source ~/qlds/.venv/bin/activate` anytime, and then run your `pip install <library>` as you like.

- Install maturin into the virtual python environment:
```shell
pip install maturin
```

- Install shinqlx from pypi source distribution into the virtual python environment (this may take a while):
```shell
pip install shinqlx
```

- Since this may take a while, if you want to see the current progress use run in verbose mode:
```shell
pip install -v shinqlx
```

- Copy the convenience script `run_server_shinqlx.sh` into `~/qlds`, or whatever other directory you might have installed the quake live dedicated server files in. (Note: The remaining sections assume you installed the dedicated server into ~/qlds)
```shell
cp run_server_shinqlx.sh ~/qlds/
```

Note: In your server startup script, you will have to also activate the the python virtual environment where the different python libraries are installed. I recommend writing a custom script that can be started with `supervisord` later on, that calls run_server_shinqlx.sh, i.e.:
```shell
#!/bin/bash
basepath="$(dirname $0)"
gameport=`expr $1 + 27960`
rconport=`expr $1 + 28960`
servernum=`expr $1 + 1`

source $basepath/.venv/bin/activate

exec $basepath/run_server_shinqlx.sh \
+set fs_basepath $basepath \
+set net_strict 1 \
+set net_port $gameport \
+set fs_homepath /home/steam/.quakelive/$gameport \
+set zmq_rcon_enable 0 \
+set zmq_rcon_password "<a super secret rcon password that no one will ever guess>" \
+set rmq_rcon_port $rconport \
+set zmq_stats_enable 1 \
+set zmq_stats_password "<a super secret stats password that sites like qlstats.net will need to know to gather stats from your server>" \
+set zmq_stats_port $gameport
```

`run_server.sh 0` will start a server on port 27960, `run_server.sh 1` on port 27961, and so on.

# Supervisor configuration
I recommend running your ShiNQlx server through a process monitor like `supervisor`. Unfortunately, due to the nature Rust handles crashes with its own panic system, some crashes may not result in the server exiting properly for the supervisor daemon to proper restart your server. Until I can come up with a proper way to solve this, I recommend to add an event listener configuration to your supervisor configuration to automatically check whether your server port is still reachable, and restart the server automatically if it's not.

The [`supervisor_checks` package](https://github.com/vovanec/supervisor_checks) provides the main means for this. However, you will have to install the package to your OS pip environment, like so:
```shell
sudo pip3 install --break-system-packages supervisor supervisor_checks
```

After installation, you can configure your server in the supervisor configuration. Here is an example `/etc/supervisor/conf.d/quakelive.conf`, running two servers on the same host, and the supervisor_checks checking every 60 seconds for connectivity to the respective port (27960 and 27961) and restarting the server in case it stopped responding to TCP connection attempts on the server port:
```ini
[program:quakelive]
command=/home/steam/bin/run_server.sh %(process_num)s
user=steam
process_name=qzeroded_%(process_num)s
numprocs=2
autorestart=true

[eventlistener:ql_heartbeat_0]
command=/usr/local/bin/supervisor_tcp_check -N "quakelive:qzeroded_0" -n ql_heartbeat_0 -r 1 -p 27960
events=TICK_60

[eventlistener:ql_heartbeat_1]
command=/usr/local/bin/supervisor_tcp_check -N "quakelive:qzeroded_1" -n ql_heartbeat_1 -r 1 -p 27961
events=TICK_60
```
