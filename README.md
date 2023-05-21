# shinqlx
ShiN0's Quake Live eXtension, implemented in Rust. Most functionality from [minqlx](https://raw.githubusercontent.com/MinoMino/minqlx) should work, if you provide the python files from minqlx in its minqlx.zip file. Support for Python 3.7 and above should work out of the box.

Some limitations apply for certain minqlx functions maybe used in plugins.
* minqlx.replace_items is not implemented at all. I doubt there are plugins out there using these functions.
* 32-bit implementation may not work. It's untested.
* Some compatibility might not work, as this implementation is not yet fully tested.

# Compilation and installation
Install rust and cargo, run cargo build and copy libshinqlx.so from target/debug or target/release (if you built with cargo build --release) and run_shinqlx_server.sh over to your qlds installation folder. Run the server through the shell-script run_shinqlx_server.sh.

By default, the python embedding and python dispatchers will be reflected from the minqlx C implementation. If you run cargo with --no-default-features, you will get a native rust implementation, rather than the minqlx C implementations.
