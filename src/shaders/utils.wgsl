fn hsl2rgb(hsv: vec3<f32>) -> vec3<f32> {
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

