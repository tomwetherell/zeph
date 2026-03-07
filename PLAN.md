As a machine learning engineer in weather and climate, I often work with large datasets in `zarr` format.
There is currently no easy way to quickly inspect a zarr store, to understand the variables, dimensions, chunking, etc. Often, the easiest way is
to open the `zarr` store using `xarray` in a Python notebook or script, and inspect with `xarray commands`.

This project, `zeph`, is to develop a CLI tool to make inspecting `zarr` stores quick and easy.

The ergonomics of `zeph` should be similar to Claude Code. Users will start `zeph` by either changing directory to a local `zarr` store and running `zeph` in their terminal, or by using
`zeph <path to local or remote zarr store>`. Like with Claude Code, once `zeph` has been started the user will have access to commands.

## Commands

We will start with a very limited set of commands, and build from there.

### Summary

Print an xarray-style summary of the `zarr` store.

Command: `/summary`

### Help

Show help and available commands.

Command: `/help`

## Tools

- Rust
- `zarrs`
  - A Rust zarr library (see https://github.com/zarrs/zarrs)
- `object_store`
  - To handle `zarr` stores in the cloud

## Development

- The project will use Claude Code for development.
- Use Rust best-practises
