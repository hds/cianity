# Cianity

_Add a bit of sanity to your CI._ Cianity (pronounced _sanity_) is a set of tools for writing CI workflows using the ciane (pronounced _sane_) DSL. Ciane workflows can then be built to a target CI platform's CI definition format. Currently only [GitLab pipelines] are supported.

[GitLab pipelines]: https://docs.gitlab.com/ci/pipelines/

It's all very early days still, so things may not work as intended in all cases as the support for workflow features is still pretry minimal.

> [!WARNING]
> The implementation of Cianity has made significant use of AI assisted coding. I know that this is a deal-breaker for some people, so I want to be up front about it. For me, it has been the difference between Cianity existing at all and not existing (which is what has happened for the last couple of years since I had the idea).

Having said that, the documentation (such as it is) is completely written by hand.

## Quick Start

Everything is pre-release right now, so first you'll need to clone this repo and build with `cargo`. If you don't have a Rust toolchain installed, then head to [rustup.rs] to remedy that situation.

[rustup.rs]: https://rustup.rs/

```sh
cargo build --release --bin cianity
```

The executable can be found in `target/release/cianity`.

Follow the links to get started with the [`cianity`] command line tool and the [Ciane] DSL.

Install the Ciane language plugin for [Vim/NeoVim] or [VSCode].

[`cianity`]: ./cianity/command_line_tool.md
[Ciane]: ./ciane/language_guide.md
[Vim/NeoVim]: ./editors/vim.md
[VSCode]: ./editors/vscode.md

## Why?

Why would you want to write in some other format, just to end up with the same configuration files at the end of the process? GitLab's pipelines are described in YAML. We all know that "YAML Ain't Markup Language", but YAML Ain't Code Either. We were sold Infrastructure as Code, and what we got was Infrastructure as Config files, the acronym is the same, but it's not the same.

On the other hand, Ciane **is** code. So you get the nice things we've become accustomed to, a linter, a formatter, keyword syntax highlighting, LSP support with auto-completion, jump to definition, and references capabilities. As of today, there are plugins for [Vim/NeoVim] and [VSCode].

Ciane workflows give you explicit templates that jobs can inherit from including cross-file imports. The output GitLab pipeline configuration **doesn't** use `extends`, the configuration for every job is right there in the job so that you don't have to go hunting across different files to work out what's going on.

For people who don't write CI workflows every day, remembering convensions and every key used in a YAML map just isn't feasible. So Ciane workflows prefer configuration over convension and the IDE plugins auto-conplete attributes for you (although the current implementation is lacking).

## Example

Here's a simple Ciane workflow that's used right here in this repository ([`workflow.ci`]):

```ciane
workflow main ( strategy = default_branch_and_reviews )

template rust_slim ( image = rust:1.96-slim-trixie )

stage build {
    template build (
        inherit = rust_slim,
        variables = ( RELEASE_FLAG = "" ),
    ) { cargo build $RELEASE_FLAG --workspace }

    job build_debug ( inherit = build ) [
        steps,
    ]

    job build_release (
        inherit = build,
        variables = ( RELEASE_FLAG = --release ),
    ) [
        steps,
    ] -> [ target/release/cianity ]
}

stage test {
    job test ( image = rust:1.96-trixie ) [
        step install_nextest { curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin }
        step run_tests { cargo nextest run --workspace }
    ]

    job fmt ( inherit = rust_slim ) [
        step install_rustfmt { rustup component add rustfmt }
        step fmt_check { cargo fmt --check }
    ]

    job clippy ( inherit = rust_slim ) [
        step install_clippy { rustup component add clippy }
        step clippy { cargo clippy }
    ]
}

stage cianity_check {
    job check (
        image = rust:1.96-trixie,
        dependencies = [ build.build_release ],
    ) [
        step check { ./target/release/cianity check }
        step format { ./target/release/cianity format --check }
        step build { ./target/release/cianity build -t gitlab --check }
    ]
}
```

[`workflow.ci`]: https://github.com/hds/cianity/blob/main/workflow.ci

For a breakdown of this example, see [Cianity's Own Workflow] under [Examples].

[Cianity's Own Workflow]: ./examples/cianity_workflow.md
[Examples]: ./examples.md
