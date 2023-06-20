# shinqlx
![python](https://img.shields.io/badge/python-3.8%7C3.9%7C3.10%7C3.11-blue.svg)
![Tests](https://github.com/mgaertne/shinqlx/actions/workflows/test.yml/badge.svg)
[![codecov](https://codecov.io/gh/mgaertne/shinqlx/branch/main/graph/badge.svg?token=VK9QI52BZX)](https://codecov.io/gh/mgaertne/shinqlx)

ShiN0's Quake Live eXtension, implemented in Rust. Most functionality from [minqlx](https://raw.githubusercontent.com/MinoMino/minqlx) should work, if you provide the python files from minqlx in its minqlx.zip file. Support for Python 3.8 and above should work out of the box.

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
sudo apt-get -y install redis-server git build-essential
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

- Clone this repository and compile shinqlx (unfortunately this might take a long while if you're compiling for the first time and recently ran `cargo clean`)

```shell
git clone https://github.com/mgaertne/shinqlx.git
cd shinqlx
maturin -Z build-std=std build --release
```

- Install the generated python wheel into the virtual python environment:
```shell
pip install target/wheels/shinqlx*.whl
```
- 
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
