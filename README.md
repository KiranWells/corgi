# Corgi

Corgi is a performance-focused fractal rendering engine capable of ultra-deep and highly flexible styling.

![An example of the UI in use at extreme zoom levels](assets/ui_example_small.avif)

## Features

### High precision rendering

Corgi uses a combination of several precision-extending techniques to render views of the Mandelbrot set at nearly infinite zoom levels. So far, the deepest rendered image had over $10^{250}$ times magnification.

### Performance Optimizations

Corgi uses optimized algorithms and efficient hardware utilization to render images as fast as possible on your hardware, often achieving real-time interactive rendering. Features include:

* Parallelized rendering in GPU compute shaders
* Fine-grained caching to avoid re-rendering when unnecessary
* Immediate UI feedback combined with asynchronous re-rendering

### Highly Customizable Image Styling

Corgi includes several coloring algorithms and a layer-based compositing system to allow an incredible amout of variation even for the same fractal locations.

![A collage of several different styles applied to the same fractal location](assets/style_collage.avif)

### Planned Features

Corgi is still in alpha, so there are many more features I still plan to add. Until version 1.0, the exact algorithm and save file format may have breaking changes.

### Know Issues

Above about $10^{90}$ times zoom, some visual artifacts appear around some Mini-brots. In Julia mode, artifacts appear much sooner. This seems to be due to a rounding issue in the iteration algorithm.

Internal coloring algorithms are unstable at high zoom levels, and internal distance estimation is not implemented yet.

## Usage

Currently, binary releases are not being created. However, you can install Corgi by cloning the repository and running:

```bash
cargo install --path .
```

This will compile a binary named `corgi`.

Basic CLI options can be viewed with `--help`:

```bash
corgi --help
````

## Troubleshooting

If the application fails to load, it likely encountered an issue during GPU initialization. Open up an issue, and I will see what I can do to support your machine. Currently, I have tested on dedicated GPUs on the Vulkan backend.

To get more information about what is happening, you can set the `CORGI_LOG_LEVEL` environment variable:

```bash
# run with the highest level of detail
CORGI_LOG_LEVEL=TRACE cargo run --release
```
