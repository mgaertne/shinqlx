# Protecting your server against query floods

Quake Live servers answer Steam server queries (the `A2S_INFO` family) on their game port. A query is a tiny UDP
packet, the answer is considerably larger, and UDP source addresses can be forged. An attacker can therefore make your
server send answers to a victim's address (reflection and amplification), or simply drown the server in requests. This
page describes what shinqlx can do about it and how to combine it with a firewall.

No single layer is enough. Use them together:

| Layer                     | Where it acts                    | Protects against                                         | Needs                                  |
|---------------------------|----------------------------------|----------------------------------------------------------|----------------------------------------|
| Upstream mitigation       | your hoster's network            | attacks that saturate the connection                     | a hoster that offers it                |
| XDP/eBPF filter           | network driver, before the stack | packet floods, many queries, malformed packets           | root, driver or hoster support, effort |
| nftables rule             | kernel firewall                  | repeated queries from one source                         | root                                   |
| Rcon/stats port limits    | server cvars, kernel firewall    | password guessing, connection floods on the TCP ports    | root for the firewall rules            |
| shinqlx query rate limit  | inside the server process        | queries that reach the server, reflection from one source | building shinqlx with a Cargo feature  |

Only the upstream layer helps against attacks that fill your uplink. Everything on the host itself, including shinqlx,
only protects what still arrives at the machine.

## The shinqlx query rate limit

The limiter is part of the Cargo feature `steam-query-limit`, which is enabled by default. To build without it:

```shell
maturin build --release --no-default-features --features alloc
```

`--features` is handed through to the Rust build, so several features can be combined as a comma-separated list.
`alloc` is the other default feature and keeps the custom memory allocator.

### What it does

The Quake Live server hands connectionless packets to the Steam game server API, which produces the answers to server
queries. shinqlx hooks the engine function that does this handover and counts the packets before Steam sees them:

* Only query requests are counted: packets that start with `FF FF FF FF` followed by `T` (`A2S_INFO`), `U`
  (`A2S_PLAYER`) or `V` (`A2S_RULES`). All other packets are passed on untouched, so master server traffic and
  authentication cannot be starved by a flood.
* Each IPv4 source gets its own token bucket. A second bucket limits all sources together, which holds back floods with
  many (forged) source addresses.
* A request over the limit is reported as handled and never reaches Steam. No answer is sent.
* The per-source state lives in a fixed table of 2048 slots without heap allocation, so a flood from many addresses
  cannot grow the memory of the server. Sources that land on the same slot share one bucket.
* Quake Live itself only has IPv4 addresses in its network layer, so there is nothing to limit for IPv6.

If the engine function cannot be found, for example after a server update, shinqlx logs
`Steam query handler not found. The server is NOT protected against query floods.` and continues without the limiter.

### Configuration

The limits are cvars. They are created with their defaults when shinqlx starts, unless your `server.cfg` already sets
them, and they are re-read every five seconds, so changes apply without a restart.

| Cvar                       | Default | Meaning                                                        |
|----------------------------|---------|----------------------------------------------------------------|
| `qlx_steamQueryRate`       | `1`     | queries per second one IP address may sustain (`0` = no limit) |
| `qlx_steamQueryBurst`      | `10`    | queries one IP address may send in a quick succession          |
| `qlx_steamQueryGlobalRate` | `10`    | queries per second answered for all addresses together (`0` = no limit) |
| `qlx_steamQueryRulesRate`  | `0.1`   | rules queries per second one IP address may sustain (`0` = no limit of its own) |
| `qlx_steamQueryRulesBurst` | `2`     | rules queries one IP address may send in a quick succession (a complete query takes two) |
| `qlx_steamQueryRulesGlobalRate` | `10` | rules queries per second answered for all addresses together (`0` = no limit of its own) |
| `qlx_steamQueryDropRules`  | `0`     | `1` drops the rules query (`A2S_RULES`) without an answer; server browsers use the info query |

The defaults follow the limits ioquake3 uses for `getstatus` and `getinfo` (burst of 10, one request per second per
address, ten per second overall). They are strict. If a server list site or a monitoring tool polls your server
regularly, you may want to raise them, for example:

```
set qlx_steamQueryRate 5
set qlx_steamQueryGlobalRate 100
```

To switch the limiter off completely, both rates have to be `0`; `qlx_steamQueryRate 0` alone leaves the global limit
active.

The rules query has the largest answers of the three queries and is rarely needed, so it has stricter limits of its own
(one complete query per twenty seconds and address, ten packets per second overall). A rules query has to pass these and the general limits;
a rules query dropped by its own limits does not use up the general ones. `qlx_steamQueryDropRules 1` drops rules
queries altogether; tools that read the rules of your server stop working while it is set. It is off by default. Like
the limits, all of these are re-read every five seconds.

While packets are dropped, shinqlx writes a summary like `Dropped N Steam query packets that exceeded the rate limit.`
to the log, at most once per minute.

### Trying it out

Test from a different machine than the server, since loopback traffic and local firewall rules distort the result, and
only against your own server. Send queries at a fixed rate from one address and count the answers. With the defaults,
one address should get about the burst plus one answer per second: a test of 100 queries per second over ten seconds
yields roughly twenty answers. With both rates set to `0` all of them should be answered. If you run an nftables rule
like the one below as well, it will limit the result too; take it out for the measurement.

## nftables

An nftables rule can limit repeated queries from one address. This one matches the standard `A2S_INFO` request
(`FF FF FF FF`, `T`, `Source Engine Query`, terminating zero) on the game ports and drops everything above 15 per
minute per source after a burst of 30:

```
#!/usr/sbin/nft -f

table inet ql_filter {
    set a2s_flood_v4 {
        type ipv4_addr
        flags dynamic, timeout
        timeout 90s
    }

    chain input {
        type filter hook input priority 0; policy accept;

        udp dport { 27960, 27961 } \
            @th,64,64 0xffffffff54536f75 \
            @th,128,64 0x72636520456e6769 \
            @th,192,64 0x6e65205175657279 \
            @th,256,8 0x00 \
            add @a2s_flood_v4 { ip saddr limit rate over 15/minute burst 30 packets } \
            drop
    }
}
```

Adjust the port list to the ports of your servers and the rate to your needs. The four raw payload matches start after
the 8 byte UDP header (bit offset 64) and cover the 25 bytes of the request, eight bytes each plus the terminating zero.
The set is dynamic and its entries expire after 90 seconds, so addresses are forgotten when they stop sending. The
kernel shows the same rule in a merged form when you list the ruleset; that is only a different notation.

On Debian, save the rules as `/etc/nftables.d/ql_filter.conf` and have `/etc/nftables.conf` include the directory:

```
#!/usr/sbin/nft -f

flush ruleset

include "/etc/nftables.d/*.conf"
```

Then load them with `sudo systemctl restart nftables` and make them permanent with
`sudo systemctl enable --now nftables`. Check the result with `sudo nft list ruleset`. Note that `flush ruleset` removes
every table that is not part of these files, including rules created by other tools such as `ufw`.

Things to keep in mind:

* The rule recognizes only this exact request. Other query types and variants of the request pass it. The shinqlx
  limiter looks at the first five bytes only and so also covers `A2S_PLAYER` and `A2S_RULES`.
* The firewall front ends on Debian (`ufw`, `fail2ban`, `iptables-nft`) use nftables as well. `sudo nft list ruleset`
  shows what is really active, whichever tool created it.
* A firewall rule drops packets before the server sees them. With both in place the stricter of the two decides how
  many answers a single address gets.
* Keep a way back before you experiment: `sudo nft list ruleset > ruleset-backup.nft` can be restored with
  `sudo nft -f ruleset-backup.nft`, and `sudo nft delete table inet ql_filter` removes just this table.

## Rcon and stats ports

The ZeroMQ rcon and stats ports (`zmq_rcon_port`, `zmq_stats_port`) are TCP ports of the Quake Live server itself. The
query rate limit of shinqlx does not look at them. The stats port is the one the stats listener of shinqlx connects
to. It may carry the same number as the UDP game port, because the protocols differ, and a rule on `tcp dport` does not
touch the game packets. Give it a number of its own anyway: a measure that filters by port number only, such as some filters of hosters or a firewall rule without a
protocol, would otherwise hit game traffic and stats connection together, and logs and counters stay separate.

These ports are served by the Quake Live server, which shinqlx cannot protect from the inside. Do not rely on the server
to slow down repeated or parallel connection attempts; put the limits in front of it, as described below. The server can
also write the rcon password into its own log and into `zmqpass.txt` in the `baseq3` directory of its home path. Keep
both private, do not post server logs, and check who can read that file.

A rcon client that disappears without closing its connection must not stop the server. The engine sends every console
line to each rcon client without a time limit and used to wait for a dead client forever, which stopped or crashed the
server. The feature `zmq-rcon-fix` (default) sets a send timeout on the rcon socket; `qlx_zmqRconSendTimeout` is the
limit in milliseconds (default `25`, about one frame at `sv_fps 40`, once per dead client). `0` makes the sends
non-blocking, a negative value keeps the unlimited wait of the engine. The cvar is read when a map is loaded.

Measured in a local test (loopback and a local proxy, no packet loss or latency): a client that is connected but does
not read stopped the server completely without the feature, and so did a client that reconnected and registered about
twenty times a second. With the feature the server kept answering in all of these cases and a fresh rcon client worked
afterwards. A client that reads normally lost no lines with `25`, `0` or the unlimited wait. A client that stops reading
holds about 2.5 MB of console output with default buffers (a steady 16 KB/s took more than four minutes to fill it);
what comes after that is discarded for this client, and the client stays registered. This is an order of magnitude from
one test setup, not a guarantee; a slow or distant connection may fill its buffers earlier.

The same tests did not stop the server through the ports without a valid password: wrong passwords, 1500 idle
connections per port and 500 unfinished handshakes had no effect. The stats port is not affected by the problem: a
subscriber that never reads, 150 subscribers and about twenty reconnects a second did not disturb the server, with
and without the feature. The frame times agree: in three runs of 120 seconds each per case (four bots, after a
minute of warm-up, builds with and without the feature), the worst frame was 26 to 28 ms without load and 27 to 32 ms
with the reconnects, and no frame took longer than 45 ms. Three more runs per case with the reconnects starting right
at the server start (build with the feature) showed the same: the worst frame in the first 30 seconds was 43 to 53 ms
(at most two frames above 45 ms) with and without the reconnects, and below 31 ms afterwards. A single earlier run
with frames of about 200 ms at the start did not repeat. Without a `zmq_stats_password` anybody who reaches the stats
port can read the event stream, which contains player names and Steam IDs, so set one unless the port stays on
loopback.

What to do about it:

1. Listen only where it is needed. `+set zmq_rcon_ip 127.0.0.1` and `+set zmq_stats_ip 127.0.0.1` make the server bind
   to the loopback address only (checked with `ss -tlnp`; without them it listens on `0.0.0.0`). Use an SSH tunnel for
   access from other machines.
2. Use long random passwords and keep them in a `server.cfg` that only the server user can read. A password on the
   command line is visible to every local user in the process list.
3. If the ports have to be reachable from outside, allow only known addresses and limit the others. The rules below
   do this for IPv4; IPv6 was not looked at. Replace the ports with the `zmq_rcon_port` and `zmq_stats_port` of each
   server, and `192.0.2.10` with your own address:

```
#!/usr/sbin/nft -f

table inet ql_rcon_filter {
    set trusted_v4 {
        type ipv4_addr
        elements = { 192.0.2.10 }
    }

    set new_connections_v4 {
        type ipv4_addr
        flags dynamic, timeout
        timeout 2m
    }

    set open_connections_v4 {
        type ipv4_addr
        flags dynamic
    }

    chain input {
        type filter hook input priority 0; policy accept;

        tcp dport { 28888, 28889 } ip saddr @trusted_v4 accept
        tcp dport { 28888, 28889 } ct state new \
            add @new_connections_v4 { ip saddr limit rate over 6/minute burst 10 packets } \
            drop
        tcp dport { 28888, 28889 } ct state new \
            add @open_connections_v4 { ip saddr ct count over 5 } \
            drop
    }
}
```

Trusted addresses pass unchecked, every other address may open six new connections per minute (after a burst of ten) and
hold five at the same time. This is tight for tools that reconnect often from one address, so put those addresses into
`trusted_v4`.

The rules were loaded and tried in a private network namespace without root rights (`unshare -Urn`): from one source
address, 5 of 20 simultaneous connections were accepted and further ones were refused once the rate was used up.
They were not tried on a real server. Test them on your own machine first and keep a way back as described above.

## XDP

XDP lets an eBPF program decide about a packet in the network driver, before the kernel network stack has done any work.
Dropping a packet there is much cheaper than in nftables, which makes it the right tool when a flood is large enough to
hurt the CPU of the machine even though your link is not saturated. It does not help against attacks that saturate the
link itself, only your hoster can.

shinqlx does not ship an XDP program and the layers above do not depend on one. If you write or deploy your own, these
notes may help.

Practical notes before you write or deploy one:

* **Driver support.** The best performance needs native mode, which depends on the network driver. On a virtual server
  the virtio network driver supports it; see the test below for a machine with a single queue. The generic mode works
  almost everywhere, but runs later in the stack and is slower. Check what your interface supports.
* **Tooling.** The `xdp-tools` package (with `libxdp`) and `bpftool` are the usual ways to load and inspect programs.
  The `xdp-filter` utility from `xdp-tools` offers static allow and deny lists for addresses and ports; it does not
  rate limit.
* **`PATH`.** On Debian, `/usr/sbin` is not in the `PATH` of a normal user, so `ethtool`, `ip` and `bridge` report
  "command not found". Run them with `sudo` or with the full path, for example `/usr/sbin/ip`.
* **Attaching and detaching.** `ip link set dev <interface> xdp obj <program.o> sec xdp` loads a program,
  `ip link set dev <interface> xdp off` removes it. Add `xdpgeneric` or `xdpdrv` to choose the mode.
* **Do not lock yourself out.** A broken filter can drop your own SSH session. Test on a console you can reach without
  the network, let SSH pass unconditionally, and start in generic mode.
* **MAC-based limits.** On a virtual server the Ethernet source address usually belongs to the next router of the
  hoster, not to the remote sender. Limit by IP address.
* **Rates.** Pick limits from your own traffic. Legitimate server browsers and query tools may send a few queries per
  second, so a per-source limit that is much looser than the shinqlx defaults can be right for a filter that sits in
  front of the server.

### Checking whether XDP works on your server

The following steps were run on one virtual server with Debian 13, kernel 6.12, two vCPUs and a `virtio_net` interface
with a single queue (`eth0`). They attach `xdp-filter` without any rules, which passes all traffic, so nothing is
dropped. Adjust the interface name. What was not tried is listed at the end.

1. Install the tools (Debian 13 provides `xdp-tools` 1.5, `bpftool` 7.5, `libbpf-dev` and `clang` 19):

   ```shell
   sudo apt update
   sudo apt install ethtool xdp-tools bpftool libbpf-dev clang llvm linux-libc-dev
   ```

2. Look at the interface. These commands only read:

   ```shell
   sudo ethtool -i eth0
   sudo ethtool -l eth0
   sudo ethtool -k eth0 | grep -Ei 'large-receive|generic-receive|tcp-segmentation|rx-gro'
   sudo bpftool net
   mount | grep bpf
   ```

   `ethtool -i` names the driver, `ethtool -l` the number of queues, and `bpftool net` must show nothing attached to
   the interface. The bpf file system should be mounted at `/sys/fs/bpf`. A running `tcpdump` keeps the interface in
   promiscuous mode, which does not matter for XDP.

3. Set a way back before you attach anything. The two timers detach whatever is attached after three minutes; keep a
   console of your hoster that works without the network, and a second SSH session open:

   ```shell
   sudo systemd-run --on-active=3m --unit=xdp-off-drv /usr/sbin/ip link set dev eth0 xdp off
   sudo systemd-run --on-active=3m --unit=xdp-off-skb /usr/sbin/ip link set dev eth0 xdpgeneric off
   ```

   After the test stop them with `sudo systemctl stop xdp-off-drv.timer xdp-off-skb.timer`. Set them again for every
   new test.

4. Attach in generic mode, look at the result and detach again:

   ```shell
   sudo xdp-filter load --mode skb eth0
   sudo xdp-filter status
   sudo bpftool net
   sudo xdp-filter unload eth0
   ```

   `bpftool net` lists the program as `generic` on the interface; after `unload` it is empty again. A second `unload`
   reports that `xdp-filter` is not loaded, and `xdp-filter status` in that state complains that it cannot find the
   stats map. Both are harmless.

5. Repeat with `--mode native`. On the test machine this worked, `bpftool net` showed the program as `driver`, and ping
   and SSH sessions stayed up. The kernel log (not the console) then contained
   `XDP request 3 queues but max is 1. XDP_TX and XDP_REDIRECT will operate in a slower locked tx mode.` Check it with
   `sudo dmesg | tail`. With a single queue only the actions that send packets back out are slower; passing and
   dropping are not affected, which is all a filter in front of the server needs. `rx-gro-hw` was on and did not block
   the attach.

Attaching in native mode may interrupt reception for a moment, so do it when no players are on the server.
`tcpdump` does not see packets that XDP dropped. To measure what a filter does, read the counters of `xdp-filter` or
`bpftool` instead.

Not tried: real filter rules, drops under load, behaviour while a match is running, your own XDP program built with
`clang` and `libbpf`, and loading a program permanently at boot. Treat everything beyond the steps above as untested on
your machine until you have checked it.

## Probes that look odd but are harmless

Capturing the traffic of a server for a day shows more than players and server lists. One pattern turned up that looks
like an attack at first glance and is not:

* A single address sends an `A2S_INFO` request (`FF FF FF FF`, `T`, `Source Engine Query`) that is padded with zero
  bytes to a UDP payload of 1200 bytes, with the don't-fragment flag set. The packets come in pairs about 1.5 seconds
  apart, to each game port in turn, roughly one pair per port every ten minutes, around the clock, with changing source
  ports.
* The request is larger than any answer the server gives, so it cannot amplify anything, and the rate is far below
  anything that costs CPU. It looks like a scanner or monitoring tool that checks how a server reacts to large queries
  or what the path allows. Whose it is, the capture does not tell.

There is no reason to block such a source. The size limit for large packets of an XDP filter counts these packets
(`limit_large`) and is the right place to shrug them off. They are worth knowing when you read a capture, so that they
do not hold you up the next time.
Regular `A2S_INFO`, `A2S_PLAYER` and challenge requests from a handful of addresses every few seconds, around the clock,
are the same kind of background noise: server list crawlers and monitoring.

A second pattern: individual addresses send a short burst of 48 packets of about 1350 bytes within one second, 24 to
each game port. The server answers each with a 14 byte packet, so nothing is amplified, and such bursts came only a few
times a day.

### What 48 hours without any filter looked like

One server was recorded for 48 hours with XDP and nftables switched off, both game ports open, headers only. It is a
small server and was mostly empty, but it is a useful yardstick for what ordinary internet noise looks like:

* About 430000 packets in both directions, 2.5 per second on average. Nothing was dropped by the kernel.
* About 4000 distinct sources. The three or four busiest were crawlers and monitoring with 0.1 to 0.2 packets per second
  each, constant day and night.
* The highest rate was 125 packets per second for seven minutes, from one address: a player joining and playing. No
  other address exceeded 53 packets per second in any single second.
* The server's frame times stayed clean (p99 below 28 ms at a 25 ms frame, no stalls outside the join of a player), and
  the XDP counters taken beforehand showed no bans and no rate limit hits except `limit_large` and `limit_rules`.

A server like this was not under attack. The counters of your own filter and a day of `tcpdump -w` with a short snap
length (`-s 96`, a rotating set of files) are enough to tell the difference, as long as you read the per-second rate of
the busiest source and not just the total.

## Floods that fill the line

Everything described here acts on the host, after the packets have arrived. If an attack sends more traffic than your
connection can carry, the line is full before XDP, nftables or shinqlx see a single packet, and none of them can help.
Only measures upstream of your connection do:

* DDoS protection of your hoster, which filters or diverts attack traffic inside its network. Ask what it covers, since
  offers differ in what they detect and how fast they react.
* A filtering service or firewall in front of the server that receives all traffic and forwards only the clean part.
* As a last resort, a new IP address, which an attacker who targets the address finds again only with some effort.

The measures on the host stay useful for attacks below the capacity of the line, which still cost CPU, and for
reflection from your server onto a third party. They are the layer behind the upstream one, not a replacement for it.

## Limits of this documentation

The limiter was developed against the Quake Live dedicated server build available when it was written and checked in
tests and in live runs with floods from a single address. The behaviour against floods from many addresses is covered
by unit tests only. Server updates can move the hooked engine function; shinqlx finds it by byte pattern and reports in
the log when that fails.

Weaknesses of the dedicated server itself that a setting or a firewall rule cannot remove are not described here. If you
find one, report it as described in [SECURITY.md](SECURITY.md) and not as a public issue.
