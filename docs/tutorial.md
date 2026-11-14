# Render your first image

## Render an image

The short way, from a checkout:

```sh
make demo            # both built-in scenes at 1280x720, opened when done
make fixed-point     # just the Fixed Point scene
make 4k              # the reference scene at 3840x2160, 16 samples
make help            # every target and the variables you can override
```

Images land in `out/`. `make` runs the same container commands shown below, so it needs
`devrun` (set `DEVRUN=` if it is not at `~/git/devcontainers/bin/devrun`). Size, samples, seed
and the container's CPU limit are variables: `make fixed-point WIDTH=1920 HEIGHT=1080 CPUS=18`.

The long way:

Builds and runs happen in the shared containers through `devrun`, never on the host. Files
written under `/scratch/out/` in the container come back with `--export <name>` into a fresh
directory given by `--out`:

```sh
D=~/git/devcontainers/bin/devrun
python3 $D lumen ~/git/lumen --worktree --fetch --export reference.png --out "$(mktemp -d)" -- \
  bash -c 'mkdir -p /scratch/out && cargo run --locked --release -q -p lumen -- render --width 640 --height 360 --samples 64 --out /scratch/out/reference.png'
```

