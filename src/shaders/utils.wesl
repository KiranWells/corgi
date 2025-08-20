fn hsv2rgb(hsv: vec3<f32>) -> vec3<f32> {
    var rgb: vec3<f32>;

    let i = floor(hsv.x * 6.);
    let f = hsv.x * 6. - i;
    let p = hsv.z * (1. - hsv.y);
    let q = hsv.z * (1. - f * hsv.y);
    let t = hsv.z * (1. - (1. - f) * hsv.y);

    switch(i32((i % 6.0))){
        case 0: {rgb = vec3<f32>(hsv.z, t, p);}
        case 1: {rgb = vec3<f32>(q, hsv.z, p);}
        case 2: {rgb = vec3<f32>(p, hsv.z, t);}
        case 3: {rgb = vec3<f32>(p, q, hsv.z);}
        case 4: {rgb = vec3<f32>(t, p, hsv.z);}
        case 5: {rgb = vec3<f32>(hsv.z, p, q);}
        default: {rgb = vec3<f32>(0.0, 0.0, 0.0);}
    }

    return rgb;
}

fn rgb2hsv(rgb: vec3f) -> vec3f {
    var hsv: vec3f;
    let max = max(max(rgb.r, rgb.g), rgb.b);
    let min = min(min(rgb.r, rgb.g), rgb.b);
    hsv.z = max;
    let c = max-min;
    if c == 0.0 {
        hsv.x = 0.0;
        hsv.y = 0.0;
    } else {
        hsv.y = c / max;
        if max == rgb.r { hsv.x = ((rgb.g - rgb.b) / c); }
        else if max == rgb.g { hsv.x = ((rgb.b - rgb.r) / c) + 2.0; }
        else if max == rgb.b { hsv.x = ((rgb.r - rgb.g) / c) + 4.0; }
        hsv.x /= 6.0;
        if hsv.x < 0.0 {
            hsv.x += 1.0;
        }
    }
    return hsv;
}

fn isnan(x: f32) -> bool {
    let bits = bitcast<u32>(x);
    let exp = (bits >> 23) & 0xffu;
    let frac = bits & 0x7fffffu;
    return exp == 0xffu && frac != 0u;
}

fn isinf(x: f32) -> bool {
    let bits = bitcast<u32>(x);
    let exp = (bits >> 23) & 0xffu;
    let frac = bits & 0x7fffffu;
    return exp == 0xffu && frac == 0u;
}

