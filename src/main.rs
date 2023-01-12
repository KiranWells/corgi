use futures::executor::block_on;
// use rug::Float;
// use types::{Coloring, Image, Viewport};

// use std::env::args;
use corgi::{
    image_gen::render_thread,
    run,
    types::{GPUData, Image, PreviewRenderResources, Status},
};
use winit::{dpi::PhysicalSize, event_loop::EventLoop, window::WindowBuilder};

fn main() {
    // set up initial data
    // - message queue for image changes
    // - mutex for status
    let message_queue = std::sync::mpsc::channel::<Image>();
    let status_mutex = std::sync::Arc::new(std::sync::Mutex::new(Status::default()));

    // start render thread
    let movable = status_mutex.clone();

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(PhysicalSize {
            width: 512,
            height: 512,
        })
        .build(&event_loop)
        .unwrap();
    let gpu_data = block_on(GPUData::init(&window));
    let initial_image = Image::default();
    let render_gpu_data = block_on(corgi::image_gen::GPUData::init(
        &initial_image,
        gpu_data.device.clone(),
        gpu_data.queue.clone(),
    ));
    let preview_data = PreviewRenderResources::init(
        &gpu_data.device,
        gpu_data.surface_config.format,
        render_gpu_data.rendered_image.clone(),
        (0, 0),
    );
    let r_thread = std::thread::spawn(move || {
        block_on(render_thread(message_queue.1, movable, render_gpu_data))
    });

    // start UI thread
    block_on(run(
        message_queue.0,
        status_mutex,
        window,
        event_loop,
        preview_data,
        gpu_data,
    ));
    r_thread.join().unwrap();
    // let x_str = "-1.74999841099374081749002483162428393452822172335808534616943930976364725846655540417646727085571962736578151132907961927190726789896685696750162524460775546580822744596887978637416593715319388030232414667046419863755743802804780843375";
    // let y_str = "-0.00000000000000165712469295418692325810961981279189026504290127375760405334498110850956047368308707050735960323397389547038231194872482690340369921750514146922400928554011996123112902000856666847088788158433995358406779259404221904755";
    // // image configuration variables
    // let x = Float::with_val(x_str.len() as u32, Float::parse(x_str).unwrap());
    // let y = Float::with_val(y_str.len() as u32, Float::parse(y_str).unwrap());
    // let zoom = args().nth(1).unwrap().parse::<f64>().unwrap();
    // let max_iter = args().nth(2).unwrap().parse::<usize>().unwrap();
    // let width = args().nth(3).unwrap().parse::<usize>().unwrap();
    // let height = args().nth(4).unwrap().parse::<usize>().unwrap();
    // let image = Image {
    //     max_iter,
    //     viewport: Viewport {
    //         width,
    //         height,
    //         zoom,
    //         x,
    //         y,
    //     },
    //     coloring: Coloring {},
    // };
    // let mut use_high_precision_float = false;
    // let (device, queue) =
    //     futures::executor::block_on(image_gen::gpu_setup::setup_gpu(&mut use_high_precision_float));

    // println!(
    //     "Max buffer size: {}",
    //     device.limits().max_storage_buffer_binding_size
    // );

    // let gpu_data = image_gen::gpu_setup::create_gpu_data(device, queue);

    // if false {
    //     // use_high_precision_float && zoom > 30.0 {
    //     image_gen::request_render::<f64>(
    //         Some((image.viewport.x.clone(), image.viewport.y.clone())),
    //         &image,
    //         &gpu_data,
    //     );
    // } else {
    //     image_gen::request_render::<f32>(
    //         Some((image.viewport.x.clone(), image.viewport.y.clone())),
    //         &image,
    //         &gpu_data,
    //     );
    // }
}
