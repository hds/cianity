# Cianity's Own Workflow

This example shows the workflow that is used for Cianity's own CI pipeline, ([`workflow.ci`]).

```ciane
{{#include ../../../workflow.ci:all}}
```

[`workflow.ci`]: ./workflow.ci

Let's break it down.

The first line is the workflow definition. The strategy determines when the jobs are run (default branch and reviews - MRs - in this case).

```ciane
{{#include ../../../workflow.ci:workflow}}
```

The next section is a template, albeit a simple one. The template has the name `rust_slim` and it defines the container image to use.

```ciane
{{#include ../../../workflow.ci:template}}
```

Then we get to our first stage, the `build` stage. The name will be used in the GitLab pipeline. There are 2 jobs in this stage, which will run concurrently. They both inherit from the `rust_slim` template thst we defined earlier and specify a single command to run, `cargo build --workspace` with `--release` on the end for the `build_release` job. That release job also specifies outputs, in this case a path to an artifact, the relese build of the `cianity` binary.

```ciane
{{#include ../../../workflow.ci:stage_build}}
```

Our second stage has jobs which are a little more complex. The `test` job defines 2 steps, first `cargo-nextest` is installed and then in the second step it is used to run all our tests. Since `curl` isn't available in the slim image, this job doesn't inherit from the template, nd instead specifies the image directly.

The remaining 2 jobs in the stage also use 2 steps, first they install thr necessary rust conponent and then execute with it.

```ciane
{{#include ../../../workflow.ci:stage_test}}
```

Each step is converted into a single command line invocation in the GitLab pipeline definition.

```ciane
{{#include ../../../workflow.ci:stage_cianity_check}}
```

The final stage is the cianity chreck that could normally be placed at the beginning of a workflow to ensure that the checked in workflow is correct and that the generated GitLab pipeline configuration matches what has been checked into the repo.
