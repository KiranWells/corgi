# Road map

## App Structure

- main
    - render thread
        - probe
        - grid
        - compute
        - render
        - probe + grid can be concurrent
        - only render needs to be redone if only coloring changed
        - only grid+ needs to be redone if the probe did not change
    - main event loop (thread)
        - UI rendering code - abstract into its own section, provide as input state a ref to the image parameters - maybe replace the float w/ a string.
            - Preview showing code - needs a reference to the image texture (maybe can own it? or is it needed? just use the bind groups)
                - handles events that change viewport
            - settings management - changes parameters, sends messages (debounce here or in render thread)

### Data division

- render thread
    - gpu data:
        - device ref and queue ref (maybe create a separate one for this thread)
        - buffers and bind groups for the pipelines
        - compute pileline
        - render pipeline layout
    - other data
        - previous rendered image
        - progress ref (atomic int?)
        - signal sender/receiver
            - receives new image descriptions
            - sends progress reports (or uses a string mutex)
- UI data
    - rendering data
        - device/queue
        - egui states
        - preview rendering states
            - buffers, pipeline
            - uniform - angle, scale, offset
    - ui state
        - image config
            - settings editor
                - viewport, max iter
                - coloring
            - preview
                - viewport
        - status
            - string - status message
            - severity - info, error
            - progress - option(float/atomic int)

## Image Generation

### Research

https://gbillotey.github.io/Fractalshades-doc/math.html
- precision loss fixes
- Billinear approximations
- Extended range floating points
- Finding minibrots
    - selecting new probe points
    - finding interesting locations to go to

### Features

- [x] distance estimation and smoothed step
    - requires adding a derivative estimation to the probe, delta generation, and calculation
    - delta_n+1' = 2X_n delta_n' + 2X_n'delta_n + 2delta_n delta_n'
    - delta_0' = 0
- [ ] interior distance estimation
    - requires second order derivative?
- [ ] billinear approximations to accelerate initial rendering?
- [-] add multiple probes to reduce errors in image generation
    - precision loss fixes or re-selecing probe point is better
- [ ] multiple precision options:
    - [ ] 32-bit direct calculation
    - [ ] 64-bit floating precision - requires a shading language other than wgsl (unless the extension gets supported)
    - [x] 32-bit probed point
    - [ ] 64-bit probed point
    - [ ] extended range floating point
        - https://andrewthall.org/papers/df64_qf128.pdf
            - could be used for f64 polyfill as well


### Performance

- [x] test using rayon for delta grid generation
- [ ] switch to GPU calculations for initial deltas, if possible
- [ ] dynamically select the work group size
- [ ] dynamic precision selection
    - use zoom level to conservatively decide on necessary precision
    - switch on precision level to decide on strategy to use:
        - either use multiple shaders or specialization constants to switch methods
        - switch:
            - < 24 - raw 32 bit render
            - < 53 - raw 64 bit render (if compatible)
            - zoom < 100 (scale ~= 1^-38) - f32 probed
            - zoom < 1000 (scale ~= 1^-308) - f64 probed (if compatible)
            - zoom < 10000 (scale ~= 1^-4900) - f128 probed
            - else - extended range fp probed

## UI

- [x] add egui, egui-wgpu, and eframe
- [x] use a custom rendering pass to render the image with transforms [see here](https://gist.github.com/zicklag/b9c1be31ec599fd940379cecafa1751b)
    - possibly have another thread doing the image render and send signals to it to copy to the final texture. render the final texture to the UI in the custom render pass whenever it needs an update

- [x] add UI
    - add a window, and set up the texture for display
    - add egui and set up a simple input box
    - listen for events on the image view to handle viewport changes
- [x] add a progress bar
    - should have a title and visual indicator of progress
    ```rs
    struct SharedState {
        status: String,
        progress: Optional<f64>,
        rendered_image: Image,
    }
    ```
- [x] Add save/load capability
    - decide on settings format (likely JSON or similar)
        - json for now, until ron is possible
        - `.corg` format specifier
    - [x] import/export text
    - [x] CLI import
    - [ ] drag and drop
- [ ] Add rendering controls
    - width, height, msaa, output location, format
- [ ] Add custom controls
    - unbounded inputs - drag up or down for adjusting (or scroll), shift for higher precision
- [?] animation support
    - allow controls to be animated from start to finish (mostly, zoom)
    - allow rendering image sequence or even video
- [ ] add styling
    - see [here](https://github.com/a-liashenko/TinyPomodoro) for reference on how
    - sketch up the desired look of the app
    - find out how to style egui

### Structure

#### Exploration View

- show the rendered image
- allow image generation values to be tweaked
    - change the x and y values as well as zoom based on drag and scroll
        - scroll into the location where the mouse is located
    - automatically set things like the max iteration count based on the zoom level
- Performance:
    - debounce re-rendering when the viewport changes, instead translating or scaling the texture to give a preview
    - Allow rendering at a 2x or 3x downscale for performance
- Quality:
    - allow changing the probe location manually
        - add an indicator of the probe's location on the image
    - automatically reprobe in the area with the highest iteration count and highest orbit value

##### Process

- User changes view - this is immediately echoed in the image settings
- debouncing starts if a compute step is needed
    - after debounce, a render starts
- the view always calculates the offset from the desired image from the currently rendered one and uses that to render

#### Coloring View

- show an image preview (full resolution - regenerate if necessary?)
- allow the coloring details to be tweaked
    - include hsv or gradient for color
    - allow brightness/hue to be tweaked by each input value
- only re-render the coloring data, using the same buffers from the previous compute step
- add node-based editing for generating the color shader

#### Rendering Menu

- allow setting image parameters such as width, height, filename, and format

## Code Cleanup

- move WGPU data initialization to at the beginning of the program, if possible, and only rebuild buffers when the image needs to be resized (width/height change)
    - add detailed descriptions of each step to ensure it is fixable in the future
    - only compile the shaders at build time to save on initialization time
- restructure data types to make more sense when the UI is added and the above refactoring is done
