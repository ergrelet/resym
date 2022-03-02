# resym

`resym` is a utility that allows browsing and extracting types from PDB files.

Inspired by [PDBRipper](https://github.com/horsicq/PDBRipper) and
[pdbex](https://github.com/wbenny/pdbex).

## Key Features

* Cross-platform
* Graphical user interface
* C and C++ types reconstruction
* Decent performance, even on huge PDB files

## Why Another PDB Dumper?

I often need to extract and analyze C++ types from 1GB+ PDB files comfortably,
in an interactive manner, but I haven't been able to find a tool that ticks all
the boxes for me so far, so this my shot at making that tool.  
So if you're in the same boat, this some tool might be of some use to you.

## How to Build

```
# Optional: install rust
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh 

$ git clone https://github.com/ergrelet/resym.git
$ cd resym; cargo build --release
$ ./target/release/resym
```
