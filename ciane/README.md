# Ciane

A Domain-Specific Language for writing CI workflows.

File extensions are typically `.ci`, but `.ciane` is also considered idiomatic.

## Index

- [Objects]
  - [`workflow`]
  - [`use`]
  - [`stage`]
  - [`job`]
  - [`step`]
  - [`step` body]
  - [`steps`]
  - [`job` output]
  - [`template`]
- [Attributes]
  - [`strategy`]
  - [`path`]
  - [`inherit`]
  - [`image`]
  - [`dependencies`]

## Objects
[Objects]: #objects

Object types in Ciane are keywords, followed optionally by key/value attributes in parentheses `( )`
and a body surrounded by braces `{ }` or for multi-step jobs brackets `[ ]`.

### Workflow
[`workflow`]: #workflow

- Parent: _none_
- Children:
  - [`stage`]
  - [`use`]
  - [`template`]
- Attributes:
  - [`strategy`]

A `workflow` is the top level object of a Ciane file. It accepts one attribute [`strategy`] which
determines when jobs in that workflow are run by default.

```ciane
workflow main ( strategy = default_branch ) {
    stage build {
        ...
    }
}
```

A `workflow`'s body braces can be ommitted, in which case the body of the `workflow` extends until
the end of the file. This reduces the indentation level on most of the file by one.

```ciane
workflow main ( strategy = default_branch )

stage build {
    ...
}
```

### Use
[`use`]: #use

- Parent: [`workflow`]
- Children: _none_
- Attributes:
  - [`path`]

In order to access [`template`]s from other [`workflow`]s, they must be specified with `use`,
providing a name by which the external [`workflow`] will be referenced and a `path` to where it is
located. 

```ciane
use deploy { path = ./ci/cianity/deploy.ci }
```

### Stage
[`stage`]: #stage

- Parent: [`workflow`]
- Children:
  - [`job`]
  - [`template`]
- Attributes: _none_

A `stage` is a group of [`job`]s which run concurrently (unless there are explicit dependencies
between them). A stage will only start running when the prior stage completes.

```ciane
stage build {
    job build_debug {
        ...
    }
}
```

### Job
[`job`]: #job

- Parent: [`stage`]
- Children:
  - [`step`]
  - [`step` body]
- Attributes:
  - [`image`]
  - [`inherit`]
  - [`dependencies`]

A `job` is a single instance of work in CI which will either pass or fail. 

A `job`'s body is a list of steps which will be executed in sequence.

```ciane
job build_debug [
    step rustup { curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh }
    step build { cargo build }
]
```

A `job` can also provide a single step with brace shorthand.

```ciane
job build_debug { cargo build }
```

The [`job`]'s attributes can be used to specify explicit [`dependencies`] on other jobs, to
[`inherit`] from a [`template`], or to specify the container [`image`] the `job` will run on.

To specify output, artifacts or variables, [`job` output] notation is used.

```ciane
job build_debug { cargo build } -> [ target/debug/bin ]
```

### Step
[`step`]: #step

- Parent: [`job`]
- Children:
  - [`step` body]
- Attributes: _none_

A `step` in a [`job`] or a [`template`] provides one or more commands to be run together.

### Step body
[`step` body]: #step-body

- Parent: [`step`], [`job`]
- Children: _none_
- Attributes: _none_

```ciane
job build_debug [
    step rustup { curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh }
    step build { cargo build }
]
```

A [`step`]'s body is surrounded by braces and can contain multiple lines of script.

```
```ciane
job build_debug [
    step rustup { curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh }
    step build {
        cargo build
        cargo build --release
    }
]
```

### Steps
[`steps`]: #steps

- Parent: [`job`]
- Children:
  - [`step`]
- Attributes: _none_

When ineriting from a template, a job may specifying `steps` in its body to use all the [`step`]s
from the template.

### Job output
[`job` output]: #job-output

The output of a job, files and environment variables are specified in a list after the job body and
the output notation arrow `->`.

```ciane
job build_config { source config.sh } -> [ build_config.json, etc/**, $MAKE ]
```

File artifacts can be soecified exactly or with globs (generally this will be passed directly to the
target pipeline configuration format.

Environment variables are prefixed with a dollar sign `$`.

### Template
[`template`]: #template

- Parent: [`stage`], [`workflow`]
- Children:
  - [`step`]
  - [`step` body]
- Attributes:
  - [`image`]
  - [`inherit`]
  - [`dependencies`]

A `template` can contain all the same attributes and body options as a [`job`], but will not be run.

Instead, a [`job`] can use the [`inherit`] attribute to inherit the attributes and body from the
template.

When using [`inherit`], if the `template` has steps, they can be reused in full with the [`steps`]
object, or only partially by name.

Consider the `template`:

```ciane
template base [
    step setup { rustup update }
    step build { cargo build }
    step run { cargo test }
]
```

This [`job`] uses all the steps as they are:

```ciane
job unit ( inherit = base ) [
    steps,
]
```

Whereas this [`job`] uses only some as well as specifying others.

```ciane
job integration ( inherit = base ) [
    step install_nextest { curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin }
    step setup,
    step build,
    step run { cargo nextest run --workspace }
]
```

## Attributes
[Attributes]: #attributes

Attributes are key/value pairs which can be attached to many objects. They are surrounded in
parentheses.

### Strategy
[`strategy`]: #strategy

- Accepted on: [`workflow`]

The `strategy` attribute on a [`workflow`] determines when jobs are run by default. The 4 options are:
- `none` (default): no rules, runs on all branches on GitLab by default
- `default_branch`: run only on the default branch
- `reviews`: run only on reviews (MRs in GitLab)
- `default_branch_and_reviews`: run on the default branch and also on reviews

### Path
[`path`]: #path

- Accepted on: [`use`]

The path to the file containing the workflow to be imported.

### Inherit
[`inherit`]: #inherit

- Accepted on: [`job`], [`template`]

Inherit attributes and/or steps from a the [`template`] named.

If the [`template`] is in another stage, then it must be specified with dot notation
`stage.template_name`. If the template is coming from another [`workflow`], then the [`workflow`]
name needs to be prefixed with a slash `external_workflow/template_name` and also with the stage if
it isn't a top level template `external_workflow/stage.template_name`.

Multiplw [`template`]s can be specified in a list.

```ciane
job build ( inherit = [ rust_slim, base ] )
```

### Image
[`image`]: #image

- Accepted on: [`job`], [`template`]

Specify the container image that the [`job`] will run on.

### Dependencies
[`dependencies`]: #dependencies

- Accepted on: [`job`], [`template`]

In order to have access to output from previously executed jobs, the `dependencies` attribute is
used to soecify a list of all jobs for which we want dependencies.

In generated GitLab pipeline, jobs that don't specify dependencies will be explicitly set with an
empty list to avoid unncessary downloads.
