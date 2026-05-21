# Ciane

A Domain-Specific Language for writing CI workflows.

File extensiosn are typically `.ci`, but `.ciane` is also considered idiomatic.

```ciane
use {
    workflow (
        location = other/dir/templates.ci
        name = good_defaults
    )
}

stage setup {
    job prepare_credentials {
         echo "USER=user" > credentials.txt
         echo "PASS=$(echo $SECRET | base64)" >> credentials.txt
    }
}

stage build (
    image = rust:1.94.0,
) {
    job build_debug (
        artifacts = ./target/debug,
    ) {
        cargo build
    }

    job build_release (
        artifacts = ./target/release,
    ) {
        cargo build --release
    }
}

stage test (
    image = rust:1.94.0,
    dependencies = [build.build_debug],
) {
    job test [
        step main {
            cargo nextest run --profile ci
        }
    ]

    template extra_tests [
        step download_artifacts {
            curl example.com/my/artifacts.gzip
        },
        step unpack {
            gunzip artifacts.gzip
        },
        step test {
            cd artifacts
            ./run_test.sh
        }
    ]

    job extra_tests_1 (
        inherit = extra_tests,
        image = container.repo.example.com/mine/extra_test:latest,
    ) [
        step download_artifacts,
        step unpack,
        step test {
            cd artifacts
            ./run_test_1.sh
        }
    ]

    job extra_tests_2 (
        inherit = extra_tests
        image = container.repo.example.com/mine/extra_test:latest,
    ) [
        steps
        step extra_validation {
            ./extra_validation.sh
        }
    ]

    job extra_tests_2 (
        inherit = good_defaults/extra_tests
    ) [
        steps
    ]
}
```

## Keywords

### `defaults`

Provides default values for the entire file.

### `stage`

A CI workflow stage.

Stages will be run sequentially.

All jobs in a stage will be run concurrently, unless there are dependencies between them.

### `job`

A single job defined by one or more steps in which commands are run sequentially.

A job's body is a list of steps surrounded by brackets (`[` `]`).

Alternatively, a job can have a single, unnamed step, in which case the job's body is denoted by the
braces (`{` `}`) for the step body.

### `step`

A step in a job which contains commands to be run in its body which is denoted by braces (`{` `}`).

### `template`

A job template. The template is never run, but can be inheritted by a job.

### `steps`

This will place all steps from the inherited template into the current job. Job must have an
`inherit` attribute.

### `workflow`

A workflow loaded from another file. It has 2 required attributes, `location` (where the template
can be found) and a `name` which is used to reference stages, jobs, or templates from the external
workflow.

## Attributes

Stages and jobs can have attributes assigned. If these are present, they will appear between
parentheses (`(` `)`) between the stage or job name and the body. Each attribute is a key/value
pair separated by an equals (`=`).

Availabile attributes are:
- `image`: runner image to use for the job or all jobs in the stage
- `dependencies`: list of other jobs that must run to completion before this job
- `inherit`: a template to inherit from
- `timeout`: the maximum time the job is allowed to run before being terminated


