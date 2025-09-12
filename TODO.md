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
https://persianney.com/fractal/fractalNotes.pdf
- explains the above in more detail
https://mathr.co.uk/blog/2021-05-14_deep_zoom_theory_and_practice.html
- describes some methods for optimizing floatexp form
https://mathr.co.uk/blog/2022-02-21_deep_zoom_theory_and_practice_again.html
- gives more detail on rebasing and a better approximation definition
https://www.fractalforums.com/general-discussion/stripe-average-coloring/msg42797/?PHPSESSID=1c149a35846c66e0c53aacdc63bc9843#msg42797
- stripe average coloring
https://iquilezles.org/articles/ftrapsgeometric/
- orbit traps
https://iquilezles.org/articles/palettes/
- generated palettes
https://web.archive.org/web/20190922070007/http://www.jussiharkonen.com/files/on_fractal_coloring_techniques(hi-res).pdf
- smooth coloring formulas
https://mathr.co.uk/web/stripe-colouring.html
- stripe averaging optimization
https://www.acsu.buffalo.edu/~adamcunn/downloads/MandelbrotSet.pdf
- internal visualization techniques
https://www.mrob.com/pub/muency/newtonraphsonzooming.html
- locating julia sets to look at

#### Other Renderers
http://www.chillheimer.de/kallesfraktaler/
- Kalles Fraktaler 2
https://mathr.co.uk/kf/kf.html
- Kalles Fraktaler 2 +, and a list of others
https://github.com/mattsaccount364/FractalShark
- has some interesting notes on high-precision CUDA floats
https://gbillotey.github.io/Fractalshades-doc/math.html#f1
- most comprehensive list of mathematics used for deep zooms

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
    - [ ] UI for selecting automatic or manual formula selector
    - [x] 32-bit direct calculation
    - [ ] 64-bit floating precision - requires a shading language other than wgsl (unless the extension gets supported)
    - [x] 32-bit probed point
        - [-] with floatexp for derivative, since it underflows sooner
    - [ ] 64-bit probed point
    - [x] extended range floating point
        - https://andrewthall.org/papers/df64_qf128.pdf
        - https://github.com/clickingbuttons/jeditrader/blob/a921a0e/shaders/src/fp64.wgsl
            - could be used for f64 polyfill as well
        - or entended exponent (floatexp):
            - (f32, i32)
- [x] use floatexp for exported data for coloring shader, since it can handle the reduced performance regardless of whether it is needed
- [?] hdr
- [ ] additional coloring algorithms
    - [x] non-smooth iteration
    - [x] smooth iteration
    - [x] distance estimation
    - [x] stripe-average coloring
    - [x] gradient coloring
    - [x] escape radii outlines
    - [x] orbit traps
        - [ ] will need some controls for point/geometry selection
    - interior:
        - [/] orbit traps
        - [ ] interior distance estimation
- [x] more flexible color formulas:
    - [x] user-defined gradient
        - [x] generated gradients
        - non-smooth/smooth iteration, stripe-average, distance estimation, gradient
    - [x] togglable overlays:
        - distance estimation, escape radii outlines
- [ ] standardized generation metadata
    - grid/viewport region
    - max iteration count
    - coloring:
        - ?
        - probably just include the parameters used internally until support for a dynamic coloring formula is added
- [ ] additional fractal algorithms
    - [x] mandelbrot
    - [ ] julia
    - [ ] burning ship


### Performance

- [x] test using rayon for delta grid generation
- [x] switch to GPU calculations for initial deltas, if possible
- [ ] dynamically select the work group size
- [x] dynamic precision selection
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
- [x] dynamic debounce delay based on render time
- [x] make sure render thread is not blocking other threads
    - cannot create separate queues in wgpu yet: https://github.com/gfx-rs/wgpu/discussions/6268

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
    - [x] rendered image metadata for reproduction    - little_exif library
- [x] Add rendering controls
    - width, height, output location, format
    - [x] explore viewport downscaling
    - [ ] anti-aliasing
- [x] ~Add custom controls~ - egui has a native drag input
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
