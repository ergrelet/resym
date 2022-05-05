# resym [![Build Status](https://github.com/ergrelet/resym/workflows/Tests/badge.svg?branch=master)](https://github.com/ergrelet/resym/actions?query=workflow%3ATests) [![rustc 1.59.0](https://img.shields.io/badge/rust-1.59.0%2B-orange.svg)](https://img.shields.io/badge/rust-1.59.0%2B-orange.svg)

`resym` is a utility that allows browsing and extracting types from PDB files.

Inspired by [PDBRipper](https://github.com/horsicq/PDBRipper) and
[pdbex](https://github.com/wbenny/pdbex).

## Key Features

* Cross-platform
* GUI and CLI versions available
* C and C++ types reconstruction
* Decent performance, even on huge PDB files

## Why Another PDB Dumper?

I often need to extract and analyze C++ types from 1GB+ PDB files comfortably,
in an interactive manner, but I haven't been able to find a tool that ticks all
the boxes for me so far, so this my shot at making that tool.  
So if you're in the same boat, this some tool might be of some use to you.

## How to Build

On **Ubuntu**, you might need to install: `libxcb-shape0-dev` and `libxcb-xfixes0-dev`.

```
# Optional: install rust
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh 

$ git clone https://github.com/ergrelet/resym.git
$ cd resym; cargo build --release
$ ./target/release/resym
```

## How to Use

If you want to use the GUI version, simply run the `resym` executable.  
A CLI version (named `resymc`) is also available:
```
$ ./target/release/resymc
resymc 0.1.0
resym is a utility that allows browsing and extracting types from PDB files.

USAGE:
    resymc.exe <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

SUBCOMMANDS:
    dump    Dump type from a given PDB file
    help    Prints this message or the help of the given subcommand(s)
    list    List types from a given PDB file
```
