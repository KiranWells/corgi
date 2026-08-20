# Styling Fractal Images

In reality, the Mandelbrot set is simply a group of numbers: a number is either part of the Mandelbrot set, or it isn't. However, this doesn't really lend itself to creating interesting images. Instead of focusing purely on the set itself, most fractal images use several different formulas to visualize the behavior of numbers within or outside of the set. Corgi provides a broad set of controls that allow you to combine and tweak these formulas to turn the raw numerical data into a compelling visualization.

If you want even more control, you can use [EXR export mode](/docs/data_export.md) to save the raw numerical data, then use your preferred software to create images, 3D models, or anything else.

## Overview

Corgi uses a layering and compositing system inspired by graphic design and 3D modeling programs, with separate systems for numbers within and outside of the set. At a high level, a style is composed of:

* Color Settings
  * [Color Mode](#color-modes)
  * [Color Layers](#layer-types)
* Lighting Settings
  * [Lighting Mode](#lighting-modes)
  * [Lighting Layers](#layer-types)
* Outlines

The **Color Settings** determine the base color of the image, **Lighting Settings** adjust how bright that color is, and **Outlines** of the set or [step edges](#step) are overlayed on top. The **Layers** take the raw numbers and convert that into a single value; the value from each layer is added together, and then the **Mode** determines how that value is turned into a color or lighting.

### Color Modes {#color-modes}

#### Flat Color

The flat mode ignores all layers and sets the whole image to a single color. Useful when most of the detail is coming from lighting or outlines.

#### Manual

Manual mode allows for creating a manual gradient. Useful for fine control over the colors in the image. This is the mode most often used in other fractal programs.

#### Procedural

This mode generates a gradient procedurally using an [algorithm created by Inigo Quiles](https://iquilezles.org/articles/palettes/). The four input "colors" are used as the parameters `a`, `b`, `c`, and `d`, and can be any value - not just colors. More info on how to adjust these parameters ccan be found in [his article](https://iquilezles.org/articles/palettes/).

#### Hue

This mode uses the HSV color space with the level value as the hue (H). Produces a rainbow effect.

#### OkLCh

This mode is similar to HSV mode, but it uses the [OkLCh perceptual color space](https://moxfield.com/decks/rKy4WvuJN0yfp-VaZzg0wg). This is designed to produce more uniform gradient of colors, but tends to be less intuitive when adjusting values. The brightest colors (cyan, magenta, yellow) tend to become washed out at brighter values due to becoming more saturated than a screen can display.

### Lighting Modes {#lighting-modes}

#### Flat Lighting

This sets the entire image to a uniform brightness. Note that other modes (such as [gradient](#gradient)) can be brighter due to how lighting is calculated.

#### Gradient {#gradient}

This directly uses the added layer values as the lighting value. Extremely bright values will shift colors towards white. If certain layers are too strong, this mode can cause the entire image to become black or white.

#### Repeating Gradient

This turns the lighting value into a repeating gradient from zero to one and back. This prevents any parts of the image from becoming over- or under-exposed.

#### Shaded

This mode uses the lighting value as a "height" and renders lighting as if looking down at a landscape from above. This mode allows for defining multiple lights with various colors, which will be mixed with a very basic shading model. This mode often gives a 3D look to the image.

## Layer Types {#layer-types}

Each layer uses one coloring formula to contribute to the color or lighting. This section describes how each of the coloring formulas work, where they come from, and how to use them to create different visual effects.

Some knowledge of the mathematics of the fractal sets being used is helpful for understanding the behavior of each algorithm. The Wikipedia article on the [Mandelbrot set](https://en.wikipedia.org/wiki/Mandelbrot_set) and [plotting algorithms](https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set) topic are a good starting point.

### Step {#step}

Step is the simplest layer algorithm. It uses the number of iterations before a number escapes the Mandelbrot set as the value, producing a set of sharp-edged bands instead of a smooth gradient. It increases linearly.

#### Internal

Inside the set, step uses an approximate step count of the orbit the number follows, since the numbers never escape. This divides the interior into several different flat regions.

#### Sources

* [Displaying the Internal Structure of the Mandelbrot Set, Adam Cunningham (2013)](https://www.acsu.buffalo.edu/~adamcunn/downloads/MandelbrotSet.pdf)
  * Internal Orbits

### Smooth Step {#smooth-step}

Smooth step is a modified version of [Step](#step) that removes the hard boundary between the steps, and increases logarithmically.

#### Internal

Inside the set, smooth step instead uses the "Total Internal Angle" algorithm. It creates the appearance of scalloped regions within the interior.

#### Sources

* [On Smooth Fractal Coloring Techniques, Jussi Härkönen (2007)](https://web.archive.org/web/20190922070007/http://www.jussiharkonen.com/files/on_fractal_coloring_techniques(hi-res).pdf)
  * Exterior Smooth Step
* [Renormalizing the Mandelbrot Escape, Linas Vepstas (1997)](https://linas.org/art-gallery/escape/escape.html)
  * Exterior Smooth Step
* [Displaying the Internal Structure of the Mandelbrot Set, Adam Cunningham (2013)](https://www.acsu.buffalo.edu/~adamcunn/downloads/MandelbrotSet.pdf)
  * Total Internal Angle
* [Wikipedia](https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set#Continuous_(smooth)_coloring)

### Distance

The Distance mode estimates the distance to the edge of the set. It has a similar effect to [Smooth Step](#smooth-step), but creates a uniform shape surrounding the set. This algorithm is also used to create the set outline.

#### Internal

The internal distance algorithm has not been implemented yet.

#### Sources

* [Wikipedia](https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set#Distance_estimates)

### Orbit Trap {#orbit-trap}

An "Orbit Trap" tracks the nearest distance of the number to a point or shape when iterating. This produces repeating patterns of that shape around the set, and the visual effect varies depending on the shape used. Several preset shapes can be selected.

The "Offset" parameter is subtracted from the orbit value before scaling by the strength. It is useful for getting sharper details, especially in the [Gradient lighting mode](#gradient).

#### Internal

The same algorithm is used on the inside of the set, but it produces different visual effects.

#### Sources

* [geometric orbit traps, Inigo Quilez (1999)](https://iquilezles.org/articles/ftrapsgeometric/)

### Stripe Average

"Stripe Average" coloring sums up and averages a number's angle (or another component) as it iterates. It produces flowing "stripes" in Angle mode, and softer "glow"- or "blob"-like shapes in Real or Imaginary mode. It is a bit more tempermental at higher zoom levels (the Offset might be needed similarly to the [Orbit Trap algorithm](#orbit-trap) to make the detail visible), but it can produce stunning effects with the right settings.

#### Internal

The same algorithm is used on the inside of the set, but the effect is less pronounced.

#### Sources

* [On Smooth Fractal Coloring Techniques, Jussi Härkönen (2007)](https://web.archive.org/web/20190922070007/http://www.jussiharkonen.com/files/on_fractal_coloring_techniques(hi-res).pdf)
  * Various averaging algorithms
* [Fractal Forums discussion (archived)](https://web.archive.org/web/20250728235552/https://www.fractalforums.com/general-discussion/stripe-average-coloring/msg/)
* [Stripe Colouring, Claude Heiland-Allen (2024)](https://mathr.co.uk/web/stripe-colouring.html)
  * Optimized version of the algorithm

