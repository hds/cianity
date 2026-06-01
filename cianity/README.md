# Cianity

Command line tool to work with Ciane workflows.

The `cianity` command line tool contains everything there is to work with ciane workflows.

## Default workflow

Cianity will look for a file called `workflow.ci` (or `.workflow.ci` as a second choice) in the
current directory and then going up towards the root of the file system.

In this case, no file needs to be specified. The rest of this guide will assume that a `workflow.ci`
file is used.

Most commands also accept `-w/--workflow` to specify a different workflow root or `-f/--file` to
act on an individual file instead.

## Lint

To check the validity of a workflow use the `check` command.

```sh
cianity check
```

As well as ensuring that the workflow can be [built](#build), a few other warning lints are also
checked.

## Format

To format files, use the `format` command.

```sh
cianity format
```

To validate that a workspace has already been formatted (on CI for example), use the `--check` flag.

```sh
cianity format --check
```

There is no configuration for the formatting. It is what it is.

## Build

It is currently possible to build a ciane workflow to a GitLab pipeline.

```
cianity build -t gitlab
```

By default this will result in a `.gitlab-ci.yml` file which is picked up by GitLab by default.

To specify a different output file, use the `-o` flag.

```sh
cianity build -t gitlab -o main.gitlab-ci.yml
```

## LSP

Cianity comes with a built in LSP, just like a grown up programming language.

```sh
cianity lsp
```

The `lsp` sub-command will also accept the `--stdio` flag which some IDEs pass unconditionally. This
changes nothing as the cianity LSP only works over stdin/out.

## Help!

Use the `-h` flag or the `help` sub-command to see available sub-commands. The `-h` flag can be used
with sub-commands to see all available flags.
