# Security policy

## Reporting a vulnerability

Please do not open a public issue or pull request for a security problem. Use the private reporting of GitHub instead:
open the **Security** tab of this repository and choose **Report a vulnerability**
(<https://github.com/mgaertne/shinqlx/security/advisories/new>). Only the maintainers can see the report.

Include what you need to reproduce it: the version or commit, how the server is started, and what happens. A packet
capture or a log is welcome, but remove passwords (rcon and stats passwords can show up in server logs) and addresses
of third parties first.

This is a project run by volunteers, so answers come on a best effort basis.

## What is covered

* shinqlx itself, including the Python API and the native plugins.
* The tools in this repository: `ql_rcon`, `ql_stats`, `plugin_migration` and the XDP filter in `xdp/`.
* The advice in `DDOS-PROTECTION.md`, if following it leaves a server less protected than it says.

Weaknesses of the Quake Live dedicated server itself that cannot be fixed from here are best reported to the
maintainers in the same way. They are not described in this repository, so that nobody learns about an open problem from
it. If you know the contact of the vendor, tell us in the report.

## Supported versions

Fixes go into the latest release and the `main` branch.
