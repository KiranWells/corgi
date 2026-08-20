# Quick Start

The Corgi Fractal Engine is an app that lets you explore fractals (infinitely detailed patterns created by mathematical formulas), create stylized artistic visualizations of them, and render those visualizations as images (and in the future, videos). This guide will walk you through the basic steps to create and render a new fractal image.

## Start-up

When you first start up Corgi, you will be greeted by the Splash Screen. Select "New" to start fresh with the default settings.

![An example of the splash screen with the new button highlighted](./images/start_up.avif)

## Explore Mode

By default, Corgi places you in explore mode. In this mode, you can move around in the fractal with optimized settings so the UI responds quickly. Click and drag inside the viewport on the left to move around, scroll or drag with middle click to zoom in, or right click and drag to rotate. You can also select from one of the presets in the top right to instantly jump to the pictured location.

![A preview showing viewport movement inside explore mode](./images/explore.avif)

Once you have found an interesting location, select the "Style" tab in the top right to switch to start customizing how the fractal is visualized.

## Style Mode

Style mode renders a full-resolution version of your image and saves some extra data to allow you to quickly test out different looks, but this means it can take much longer to load when moving around. Most of your time in this mode will instead be spent adjusting the internal and external style settings on the right.

There are a lot of options here; too many to cover in a quick-start (see [Styling](/docs/styling.md) for a deeper dive). Instead, try using one of the included presets by selecting it from the top right menu. Not all styles will work well right away. Often you will need to adjust the strength or frequency in a few places to get the look you want.

> [!TIP]
> Hovering over the name of a setting will often give a brief description of what that setting does.

![An example of a preset when first applied vs. adjusted, with the key strength settings that needed adjusting highlighed](./images/style.avif)

After dialing in the look you want, select the "Render" tab.

## Render Mode

In this tab you can decide on the size, format, and save location of your final image. On the left is a preview that shows you what image has currently been rendered and is ready to save, along with its resolution -- this will be blank until "Render" is pressed.

![The render page, showing a rendered image and highlighting the Render and Save Image buttons](./images/render.avif)

Adjust the image resolution, then press "Render" to calculate the final image. Once complete, the final image will be shown in the viewport on the left. Finally, select "Save Image", pick a folder to save the image file in, and you're done!

## Sharing Image Settings

The final saved image has the settings used to make it stored within the file metadata, so you can open it directly in Corgi to load the exact location and style. However, this metadata is often stripped when sharing the image through online platforms. If you want to share the settings with someone, you can save them directly as a `.corg` file, or just send them the share text, like this:

```
                           /
                         <#@#>
                    \     %@K.
                     LUv_WMBTYIHd+.
           \  :.   =ABlAwAEBAAwMTA5:
            +NDc3= MDUWLTIuNzY3NjM2^
         \|MDkzNTI+4MjMjQXpEkqgjPw+
 ~~~~|~~~BA5r_KPujcdD4XoA89AgS4Hu<
         /|V56aY-g+B7wEAER_OO3r9eL+
            +OikB= CSb0hs6nxKEqH93h.
           /  :^   =y3WwVXZqbsjKj4-:
                     JTFAeSnlJya8+^
                    /     %@%^
                         <#@#>
                           \
```

To show the share text, open the menu, and click "Share".

![A screen with the share window and menu open, highlighting the Share button](./images/share.avif)

Anyone who pastes share text into the viewport window can instantly load the exact settings you shared.

## Next Steps

Learn more about the available styling settings in [Styling](/docs/styling.md).
