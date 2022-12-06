<img src="media/sumi.png" align="center"/>

Sumi is a binding generator specifically designed for [Astar Network](https://astar.network) ecosystem with XVM in mind. It takes EVM metadata and converts it to an ink! module that can later be used to call into original EVM smart contract.

Please note that Sumi is not a transpiler, it's a binding generator. If you need to convert your existing Solidity smart contract to ink! please use [Sol2ink](https://github.com/Supercolony-net/sol2ink) instead.

# Why Sumi?

すみ (墨) is a Japanese word meaning *solid ink* that is used for calligraphy and fine arts. In this project we're dealing with *Solid*idy and *ink!*, hence the name.

# Quick start guide

1. Install Sumi using `cargo install --git https://github.com/0x7CFE/sumi`
2. Use Solidity compiler (or Remix IDE if in doubt) to obtain contract metadata: `solc --pretty-json --abi <input>.sol -o .` Don't forget to replace `<input>.sol` with your actual file name.
3. `solc` should procuce file `<input>.abi` that would contain ABI JSON
4. Use the ABI file to feed Sumi: `sumi --input <input>.abi --output binding.rs --module-name <my_module>`

Sumi can also work in pipeline mode. By default it would read from stdin and write to stdout which can be handy for shell processing:

    cat IERC20_meta.json | jq '.output.abi ' | sumi -m erc20 -e 0x0F | rustfmt > erc20.rs

# Command line reference

    Usage: sumi [OPTIONS] --module-name <MODULE_NAME>
    
    Options:
      -i, --input <INPUT>              Input filename or stdin if empty
      -o, --output <OUTPUT>            Output filename or stdout if empty
      -m, --module-name <MODULE_NAME>  Ink module name to generate
      -e, --evm-id <EVM_ID>            EVM ID to use in module [default: 0x0F]
      -h, --help                       Print help information

You can always use `sumi --help` to get the same reference.

# Current limitations

Due to XVM v2 limitations currently Sumi processes only:
- functions (events are not processed)
- returning a single value `(bool)` which is currently ignored
- altering contract state, so no `view`s

Overloaded functions are supported, but their return type is also ignored for now.
