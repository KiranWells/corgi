# Exporting Raw Data

Corgi is designed to provide [a wide range of control over how a fractal is visualized](/docs/styling.md), but there are still limits to what it can do. The EXR export mode allows you to use external software to take the raw numerical values Corgi calculates and apply your own styling algorithms, or use them as a part of a separate project entirely.
 
Corgi provides a [Blender template file](https://github.com/KiranWells/corgi/blob/main/assets/exr_demo.blend) with some examples of how to use the data.

## Exporting Data

In the Render tab, switch from "Save Image" to "Export Data". Select "Render", then "Save as EXR". You will need to re-render even if you already rendered in "Save Image" mode because the EXR format includes extra data that most images don't use. Corgi skips calculating it when in Save Image mode to decrease render times.

## Using the Data

The exported EXR image is made up of several different layers, each with a different data type. Some tools don't support this very well, so you may need to use a plugin or split the EXR image into separate files.

### Layers

#### Final Color

This layer includes the rendered output image that would have been saved normally, in linear 16-bit RGB format.

#### Steps

This layer includes two 32-bit unsigned integer channels. The `x` channel contains the number of steps before the value being iterated escaped, or an approximated cycle count of the orbit if the value did not escape. The `y` channel is a 1 if the value escaped, otherwise it is 0.

#### Z

This layer contains three 32-bit float values representing the final value of the number being iterated for that pixel and the logarithmic scale of the deltas, if an algorithm using deltas was used.

#### dZ

This layer also contains three 32-bit float values, this time containing the derivative of the value being iterated. The `x` and `y` channels contain the real and imaginary components, respectively, and the `z` component is a logarithmic scale value. The actual value of the derivative can be obtained as:

```
dZ = (x * 2^z) + (y * 2^z)i
```

#### Orbit

This layer contains the four orbit shapes:
* `x` is "Center" (based on the origin)
* `y` is "Circle" (a circle of radius 2)
* `z` is "Axes" (lines at `x = 0` and `y = 0`)
* `w` is "Box" (a box with side length 4)

#### Stripe

This layer contains the three stripe values, and one empty channel reserved for future use:
* `x` is "Angle" (the averaged angle of the number being iterated)
* `y` is "Real" (the averaged real component of the number being iterated)
* `z` is "Imaginary" (the averaged imaginary component of the number being iterated)

### Computed Values

#### Smooth Step

A smoothed step can be calculated with the following formula:

```
r = ||Z||

smoothstep = step + 1 - ln(ln(r)) / ln(2)
```

#### Distance

The distace estimate can be calculated with the following formula:

```
r  = ||Z||
dr = ||dZ||

dist = ln(r) * r / dr
```
