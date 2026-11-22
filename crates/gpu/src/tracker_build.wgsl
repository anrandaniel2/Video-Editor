// Module 1 of 3 for GPU motion tracking: RGBA8 -> Rec.709 luma + crop/conform,
// 5x5 Gaussian pyramid downsampling, and 3x3 Scharr spatial gradients.

struct BuildParams {
    src_w: u32,
    src_h: u32,
    crop_x: u32,
    crop_y: u32,
    crop_w: u32,
    crop_h: u32,
    dst_w: u32,
    dst_h: u32,
    src_off: u32,
    dst_off: u32,
    pad0: u32,
    pad1: u32,
}

@group(0) @binding(0) var<uniform> ub: BuildParams;
@group(0) @binding(1) var<storage, read> rgba_words: array<u32>;
@group(0) @binding(2) var<storage, read> src_pyr: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> dst_pyr: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> luma_out: array<f32>;

fn unpack_luma(px: u32) -> f32 {
    let r = f32(px & 255u);
    let g = f32((px >> 8u) & 255u);
    let b = f32((px >> 16u) & 255u);
    return (0.2126 * r + 0.7152 * g + 0.0722 * b) * (1.0 / 255.0);
}

fn rgba_luma_clamped(ix: i32, iy: i32) -> f32 {
    let cx = clamp(ix, 0, i32(ub.src_w) - 1);
    let cy = clamp(iy, 0, i32(ub.src_h) - 1);
    let idx = u32(cy) * ub.src_w + u32(cx);
    return unpack_luma(rgba_words[idx]);
}

@compute @workgroup_size(16, 16, 1)
fn cs_rgba_to_luma(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= ub.dst_w || gid.y >= ub.dst_h {
        return;
    }
    var luma: f32 = 0.0;
    if ub.crop_w == ub.dst_w && ub.crop_h == ub.dst_h {
        luma = rgba_luma_clamped(i32(ub.crop_x + gid.x), i32(ub.crop_y + gid.y));
    } else {
        let sx = f32(ub.crop_x) + f32(gid.x) * f32(max(ub.crop_w, 1u) - 1u) / f32(max(ub.dst_w, 2u) - 1u);
        let sy = f32(ub.crop_y) + f32(gid.y) * f32(max(ub.crop_h, 1u) - 1u) / f32(max(ub.dst_h, 2u) - 1u);
        let x0 = i32(floor(sx));
        let y0 = i32(floor(sy));
        let fx = sx - f32(x0);
        let fy = sy - f32(y0);
        let a = rgba_luma_clamped(x0, y0) * (1.0 - fx) + rgba_luma_clamped(x0 + 1, y0) * fx;
        let b = rgba_luma_clamped(x0, y0 + 1) * (1.0 - fx) + rgba_luma_clamped(x0 + 1, y0 + 1) * fx;
        luma = a * (1.0 - fy) + b * fy;
    }
    let out_i = gid.y * ub.dst_w + gid.x;
    dst_pyr[ub.dst_off + out_i] = vec4<f32>(luma, 0.0, 0.0, 0.0);
    luma_out[out_i] = luma;
}

fn tap5(k: i32) -> f32 {
    let ak = abs(k);
    if ak == 0 { return 6.0; }
    if ak == 1 { return 4.0; }
    return 1.0;
}

@compute @workgroup_size(16, 16, 1)
fn cs_pyr_down(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= ub.dst_w || gid.y >= ub.dst_h {
        return;
    }
    let sw = i32(ub.src_w);
    let sh = i32(ub.src_h);
    let cx = i32(gid.x * 2u);
    let cy = i32(gid.y * 2u);
    var sum: f32 = 0.0;
    var wsum: f32 = 0.0;
    for (var ky: i32 = -2; ky <= 2; ky = ky + 1) {
        let wy = tap5(ky);
        let sy = clamp(cy + ky, 0, sh - 1);
        let row = ub.src_off + u32(sy) * ub.src_w;
        for (var kx: i32 = -2; kx <= 2; kx = kx + 1) {
            let w = wy * tap5(kx);
            let sx = clamp(cx + kx, 0, sw - 1);
            sum = sum + w * src_pyr[row + u32(sx)].x;
            wsum = wsum + w;
        }
    }
    let out_i = ub.dst_off + gid.y * ub.dst_w + gid.x;
    dst_pyr[out_i] = vec4<f32>(sum / max(wsum, 1.0), 0.0, 0.0, 0.0);
}

@compute @workgroup_size(16, 16, 1)
fn cs_scharr(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= ub.dst_w || gid.y >= ub.dst_h {
        return;
    }
    let w = ub.dst_w;
    let h = ub.dst_h;
    let idx = ub.src_off + gid.y * w + gid.x;
    let center = src_pyr[idx].x;
    if gid.x == 0u || gid.y == 0u || gid.x + 1u >= w || gid.y + 1u >= h {
        dst_pyr[idx] = vec4<f32>(center, 0.0, 0.0, 0.0);
        return;
    }
    let r0 = ub.src_off + (gid.y - 1u) * w;
    let r1 = ub.src_off + gid.y * w;
    let r2 = ub.src_off + (gid.y + 1u) * w;
    let p00 = src_pyr[r0 + gid.x - 1u].x;
    let p01 = src_pyr[r0 + gid.x].x;
    let p02 = src_pyr[r0 + gid.x + 1u].x;
    let p10 = src_pyr[r1 + gid.x - 1u].x;
    let p12 = src_pyr[r1 + gid.x + 1u].x;
    let p20 = src_pyr[r2 + gid.x - 1u].x;
    let p21 = src_pyr[r2 + gid.x].x;
    let p22 = src_pyr[r2 + gid.x + 1u].x;
    let ix = (3.0 * (p02 - p00) + 10.0 * (p12 - p10) + 3.0 * (p22 - p20)) * (1.0 / 32.0);
    let iy = (3.0 * (p20 - p00) + 10.0 * (p21 - p01) + 3.0 * (p22 - p02)) * (1.0 / 32.0);
    dst_pyr[idx] = vec4<f32>(center, ix, iy, 0.0);
}
