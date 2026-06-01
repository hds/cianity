<p align="center">
  <img alt="cianity logo" src="./assets/cianity-logo.jpg" height="200" />
</p>

# Cianity

_Add a bit of sanity to your CI._

Cianity (pronounced _sanity_) is a set of tools for writing CI workflows using the ciane (pronounced
_sane_) DSL. Ciane workflows can then be built to a target CI platform's CI definition format.
Currently only [GitLab pipelines] are supported.

[GitLab pipelines]: https://docs.gitlab.com/ci/pipelines/

It's all very early days still, so things may not work as intended in all cases as the support for
workflow features is still pretry minimal.

> [!WARNING]
> The implementation of Cianity has made significant use of AI assisted coding. I know that this is
> a deal-breaker for some people, so I want to be up front about it. For me, it has been the
> difference between Cianity existing at all and not existing (which is what has happened for the
> last couple of years since I had the idea).

Having said that, the documentation (such as it is) is completely written by hand.

## Quick Start

Everything is pre-release right now, so first you'll need to clone this repo and build with `cargo`.
If you don't have a Rust toolchain installed, then head to [rustup.rs] to remedy that situation.

[rustup.rs]: https://rustup.rs/

```sh
cargo build --release --bin cianity
```

The executable can be found in `target/release/cianity`.

Follow the links to get started with the [`cianity`] command line tool and the [Ciane] DSL.

Install the Ciane language plugin for [Vim/NeoVim] or [VSCode].

[`cianity`]: ./cianity/README.md
[Ciane]: ./ciane/README.md
[Vim/NeoVim]: ./vim-ciane/README.md
[VSCode]: ./vscode-ciane/README.md

## Why?

Why would you want to write in some other format, just to end up with the same configuration files
at the end of the process? GitLab's pipelines are described in YAML. We all know that "YAML Ain't
Markup Language", but YAML Ain't Code Either. We were sold Infrastructure as Code, and what we got
was Infrastructure as Config files, the acronym is the same, but it's not the same.

On the other hand, Ciane **is** code. So you get the nice things we've become accustomed to, a
linter, a formatter, keyword syntax highlighting, LSP support with auto-completion, jump to
definition, and references capabilities. As of today, there are plugins for [Vim/NeoVim] and [VSCode].

Ciane workflows give you explicit templates that jobs can inherit from including cross-file imports.
The output GitLab pipeline configuration **doesn't** use `extends`, the configuration for every job
is right there in the job so that you don't have to go hunting across different files to work out
what's going on.

For people who don't write CI workflows every day, remembering convensions and every key used in a
YAML map just isn't feasible. So Ciane workflows prefer configuration over convension and the IDE
plugins auto-conplete attributes for you (although the current implementation is lacking).

## Example

Here's a simple Ciane workflow that's used right here in this repository ([`workflow.ci`]):

```ciane
workflow main ( strategy = default_branch_and_reviews )

template rust_slim ( image = rust:1.96-slim-trixie )

stage build {
    job build_debug ( inherit = rust_slim ) { cargo build --workspace }

    job build_release ( inherit = rust_slim ) { cargo build --workspace --release } -> [ target/release/cianity ]
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
        inherit = rust_slim,
        dependencies = [ build.build_release ],
    ) [
        step check { ./target/release/cianity check }
        step format { ./target/release/cianity format --check }
        step build {
            ./target/release/cianity build -t gitlab
            # TODO: fix this after implementing build --check
            [ "$(git diff --name-only)" == "" ] || ( echo "error: pipeline doesn't match workflow.ci!"; exit 1 )
        }
    ]
}
```

[`workflow.ci`]: ./workflow.ci

Let's break it down.

The first line is the workflow definition. The strategy determines when the jobs are run (default
branch and reviews - MRs - in this case).

```ciane
workflow main ( strategy = default_branch_and_reviews )
```

The next section is a template, albeit a simple one. The template has the name `rust_slim` and it
defines the container image to use.

```ciane
template rust_slim ( image = rust:1.96-slim-trixie )
```

Then we get to our first stage, the `build` stage. The name will be used in the GitLab pipeline.
There are 2 jobs in this stage, which will run concurrently. They both inherit from the `rust_slim`
template thst we defined earlier and specify a single command to run, `cargo build --workspace` with
`--release` on the end for the `build_release` job. That release job also specifies outputs, in this
case a path to an artifact, the relese build of the `cianity` binary.

```ciane
stage build {
    job build_debug ( inherit = rust_slim ) { cargo build --workspace }

    job build_release ( inherit = rust_slim ) { cargo build --workspace --release } -> [ target/release/cianity ]
}
```

Our second stage has jobs which are a little more complex. The `test` job defines 2 steps, first
`cargo-nextest` is installed and then in the second step it is used to run all our tests. Since
`curl` isn't available in the slim image, this job doesn't inherit from the template, nd instead
specifies the image directly.

The remaining 2 jobs in the stage also use 2 steps, first they install thr necessary rust conponent
and then execute with it.

```ciane
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
```

Each step is converted into a single line in the GitLab pipeline definition.

```ciane
stage cianity_check {
    job check (
        inherit = rust_slim,
        dependencies = [ build.build_release ],
    ) [
        step check { ./target/release/cianity check }
        step format { ./target/release/cianity format --check }
        step build {
            ./target/release/cianity build -t gitlab
        step build {
            ./target/release/cianity build -t gitlab
            # TODO: fix this after implementing build --check
            [ "$(git diff --name-only)" == "" ] || ( echo "error: pipeline doesn't match workflow.ci!"; exit 1 )
        }
    ]
}
```

The final stage is the cianity chreck that could normally be placed at the beginning of a workflow
to ensure that the checked in workflow is correct and that the generated GitLab pipeline
configuration matches what has been checked in to te repo.

See the [ciane crate] for further details on the language.

[ciane crate]: ./ciane/README.md

To build the workflow use the `cianity` CLI tool:

```sh
cianity build -t gitlab
```

The default workspace root `workflow.ci` will be detected by `cianity` and built. The built version
of this workflow can be found in [`.gitlab-ci.yml`].


[`.gitlab-ci.yml`]: ./.gitlab-ci.yml

See the [cianity crate] for further details on the commands available.

[cianity crate]: ./cianity/README.md

## Supported Rust Versions

The Cianity crates are built against the latest stable release. The minimum supported version is
1.92. The current version is not guaranteed to build on Rust versions earlier than the minimum
supported version.

## License

This project is licensed under the [MIT license].

[MIT license]: https://github.com/hds/cianity/blob/main/LICENSE

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion
in Cianity by you, shall be licensed as MIT, without any additional terms or conditions.
