// Full GPU compute tracking pipeline for FilmCraft (2D Motion Tracking & 3D Camera Match-Move).
// Executes RGBA8 -> Rec.709 luma + crop/conform, 5x5 Gaussian pyramid downsampling, 3x3 Scharr
// spatial gradients, dense Shi-Tomasi tiled corner detection, illumination-normalized coarse-to-fine
// Inverse-Compositional Lucas-Kanade optical flow with forward-backward consistency check, coarse
// ZNCC template recovery search, and horizontal stereo disparity estimation on the GPU.

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

// ---------------- Shi-Tomasi tiled corner detector ----------------

struct CornerParams {
    img_w: u32,
    img_h: u32,
    roi_x0: u32,
    roi_y0: u32,
    roi_x1: u32,
    roi_y1: u32,
    cell: u32,
    tiles_x: u32,
    tiles_y: u32,
    lvl_off: u32,
    pad0: u32,
    pad1: u32,
}

@group(0) @binding(0) var<uniform> uc: CornerParams;
@group(0) @binding(1) var<storage, read> corner_pyr: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> out_corners: array<vec4<f32>>;

@compute @workgroup_size(64, 1, 1)
fn cs_corners(@builtin(global_invocation_id) gid: vec3<u32>) {
    let total_tiles = uc.tiles_x * uc.tiles_y;
    let tid = gid.x;
    if tid >= total_tiles {
        return;
    }
    let tx = tid % uc.tiles_x;
    let ty = tid / uc.tiles_x;
    let x_start = uc.roi_x0 + tx * uc.cell;
    let y_start = uc.roi_y0 + ty * uc.cell;
    let x_end = min(x_start + uc.cell, uc.roi_x1);
    let y_end = min(y_start + uc.cell, uc.roi_y1);

    var best_eig: f32 = 0.0;
    var best_x: f32 = 0.0;
    var best_y: f32 = 0.0;

    let win: i32 = 3;
    let max_px = i32(uc.img_w) - 1 - win;
    let max_py = i32(uc.img_h) - 1 - win;

    for (var py_u: u32 = y_start; py_u < y_end; py_u = py_u + 1u) {
        let py = i32(py_u);
        if py < win + 1 || py >= max_py {
            continue;
        }
        for (var px_u: u32 = x_start; px_u < x_end; px_u = px_u + 1u) {
            let px = i32(px_u);
            if px < win + 1 || px >= max_px {
                continue;
            }
            var gxx: f32 = 0.0;
            var gyy: f32 = 0.0;
            var gxy: f32 = 0.0;
            for (var oy: i32 = -win; oy <= win; oy = oy + 1) {
                let row = uc.lvl_off + u32(py + oy) * uc.img_w;
                for (var ox: i32 = -win; ox <= win; ox = ox + 1) {
                    let g = corner_pyr[row + u32(px + ox)];
                    gxx = gxx + g.y * g.y;
                    gyy = gyy + g.z * g.z;
                    gxy = gxy + g.y * g.z;
                }
            }
            let tr = gxx + gyy;
            let det = gxx * gyy - gxy * gxy;
            let disc = sqrt(max(tr * tr - 4.0 * det, 0.0));
            let min_eig = (tr - disc) * 0.5;
            if min_eig > best_eig {
                best_eig = min_eig;
                best_x = f32(px);
                best_y = f32(py);
            }
        }
    }
    if best_eig > 1e-3 {
        out_corners[tid] = vec4<f32>(best_eig, best_x, best_y, 1.0);
    } else {
        out_corners[tid] = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
}

// ---------------- Batched Pyramidal IC-LK + ZNCC + Stereo Disparity ----------------

struct LkParams {
    // Each vec4<u32> holds (offset, width, height, 0) for pyramid levels 0..3.
    lvl0: vec4<u32>,
    lvl1: vec4<u32>,
    lvl2: vec4<u32>,
    lvl3: vec4<u32>,
    levels: u32,
    count: u32,
    patch_r: i32,
    search_range: i32,
    fb_max_px: f32,
    max_disp: f32,
    pt_x: f32,
    pt_y: f32,
}

@group(0) @binding(0) var<uniform> ulk: LkParams;
@group(0) @binding(1) var<storage, read> prev_pyr: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> curr_pyr: array<vec4<f32>>;
@group(0) @binding(3) var<storage, read> queries: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> results: array<vec4<f32>>;

fn level_meta(l: u32) -> vec4<u32> {
    if l == 0u { return ulk.lvl0; }
    if l == 1u { return ulk.lvl1; }
    if l == 2u { return ulk.lvl2; }
    return ulk.lvl3;
}

fn sample_prev(meta: vec4<u32>, x: f32, y: f32) -> vec4<f32> {
    let w = i32(meta.y);
    let h = i32(meta.z);
    if x < 0.0 || y < 0.0 || x > f32(w - 1) || y > f32(h - 1) {
        return vec4<f32>(0.0, 0.0, 0.0, -1.0);
    }
    let x0 = clamp(i32(floor(x)), 0, max(w - 2, 0));
    let y0 = clamp(i32(floor(y)), 0, max(h - 2, 0));
    let fx = clamp(x - f32(x0), 0.0, 1.0);
    let fy = clamp(y - f32(y0), 0.0, 1.0);
    let r0 = meta.x + u32(y0) * meta.y + u32(x0);
    let r1 = r0 + meta.y;
    let a = mix(prev_pyr[r0], prev_pyr[r0 + 1u], fx);
    let b = mix(prev_pyr[r1], prev_pyr[r1 + 1u], fx);
    let v = mix(a, b, fy);
    return vec4<f32>(v.xyz, 1.0);
}

fn sample_curr(meta: vec4<u32>, x: f32, y: f32) -> vec4<f32> {
    let w = i32(meta.y);
    let h = i32(meta.z);
    if x < 0.0 || y < 0.0 || x > f32(w - 1) || y > f32(h - 1) {
        return vec4<f32>(0.0, 0.0, 0.0, -1.0);
    }
    let x0 = clamp(i32(floor(x)), 0, max(w - 2, 0));
    let y0 = clamp(i32(floor(y)), 0, max(h - 2, 0));
    let fx = clamp(x - f32(x0), 0.0, 1.0);
    let fy = clamp(y - f32(y0), 0.0, 1.0);
    let r0 = meta.x + u32(y0) * meta.y + u32(x0);
    let r1 = r0 + meta.y;
    let a = mix(curr_pyr[r0], curr_pyr[r0 + 1u], fx);
    let b = mix(curr_pyr[r1], curr_pyr[r1 + 1u], fx);
    let v = mix(a, b, fy);
    return vec4<f32>(v.xyz, 1.0);
}

fn fetch_tpl(meta: vec4<u32>, x: f32, y: f32, forward: bool) -> vec4<f32> {
    if forward {
        return sample_prev(meta, x, y);
    }
    return sample_curr(meta, x, y);
}

fn fetch_tgt(meta: vec4<u32>, x: f32, y: f32, forward: bool) -> vec4<f32> {
    if forward {
        return sample_curr(meta, x, y);
    }
    return sample_prev(meta, x, y);
}

// Single-direction pyramidal mean-normalized IC-LK: `forward = true` tracks `prev -> curr`;
// `forward = false` tracks `curr -> prev`. Returns (nx, ny, 1.0) on convergence or (0, 0, -1.0).
fn track_one_dir(p: vec2<f32>, initial: vec2<f32>, r: i32, forward: bool) -> vec3<f32> {
    if ulk.levels == 0u {
        return vec3<f32>(0.0, 0.0, -1.0);
    }
    let rf = f32(r);
    let side = 2 * r + 1;
    let inv_n = 1.0 / f32(side * side);
    let top_scale = f32(1u << (ulk.levels - 1u));
    var guess = initial / top_scale;
    var point = p;

    var l_iter: u32 = ulk.levels;
    loop {
        if l_iter == 0u {
            break;
        }
        l_iter = l_iter - 1u;
        let meta = level_meta(l_iter);
        let s = f32(1u << l_iter);
        let px = p.x / s;
        let py = p.y / s;
        if px - rf < 1.0 || py - rf < 1.0 || px + rf >= f32(meta.y) - 1.0 || py + rf >= f32(meta.z) - 1.0 {
            return vec3<f32>(0.0, 0.0, -1.0);
        }

        // Pre-pass: structure tensor Hessian + template gradient sums (zero local arrays!).
        var gxx: f32 = 0.0;
        var gyy: f32 = 0.0;
        var gxy: f32 = 0.0;
        var sum_ix: f32 = 0.0;
        var sum_iy: f32 = 0.0;
        for (var oy: i32 = -r; oy <= r; oy = oy + 1) {
            for (var ox: i32 = -r; ox <= r; ox = ox + 1) {
                let s_tpl = fetch_tpl(meta, px + f32(ox), py + f32(oy), forward);
                if s_tpl.w < 0.0 {
                    return vec3<f32>(0.0, 0.0, -1.0);
                }
                gxx = gxx + s_tpl.y * s_tpl.y;
                gyy = gyy + s_tpl.z * s_tpl.z;
                gxy = gxy + s_tpl.y * s_tpl.z;
                sum_ix = sum_ix + s_tpl.y;
                sum_iy = sum_iy + s_tpl.z;
            }
        }
        let det = gxx * gyy - gxy * gxy;
        let tr = gxx + gyy;
        let min_eig = (tr - sqrt(max(tr * tr - 4.0 * det, 0.0))) * 0.5 * inv_n;
        if abs(det) < 1e-8 || min_eig < 1e-4 {
            return vec3<f32>(0.0, 0.0, -1.0);
        }
        let h00 = gyy / det;
        let h01 = -gxy / det;
        let h11 = gxx / det;

        var gx = guess.x;
        var gy = guess.y;
        for (var it: u32 = 0u; it < 18u; it = it + 1u) {
            var raw_bx: f32 = 0.0;
            var raw_by: f32 = 0.0;
            var sum_diff: f32 = 0.0;
            for (var oy: i32 = -r; oy <= r; oy = oy + 1) {
                for (var ox: i32 = -r; ox <= r; ox = ox + 1) {
                    let tx = px + f32(ox);
                    let ty = py + f32(oy);
                    let s_tpl = fetch_tpl(meta, tx, ty, forward);
                    let s_tgt = fetch_tgt(meta, tx + gx, ty + gy, forward);
                    if s_tgt.w < 0.0 {
                        return vec3<f32>(0.0, 0.0, -1.0);
                    }
                    let d = s_tgt.x - s_tpl.x;
                    sum_diff = sum_diff + d;
                    raw_bx = raw_bx + s_tpl.y * d;
                    raw_by = raw_by + s_tpl.z * d;
                }
            }
            let mean_d = sum_diff * inv_n;
            let bx = raw_bx - mean_d * sum_ix;
            let by = raw_by - mean_d * sum_iy;
            let vx = -(h00 * bx + h01 * by);
            let vy = -(h01 * bx + h11 * by);
            if abs(vx) > 16.0 || abs(vy) > 16.0 {
                return vec3<f32>(0.0, 0.0, -1.0);
            }
            gx = gx + vx;
            gy = gy + vy;
            if abs(vx) + abs(vy) < 0.004 {
                break;
            }
        }

        // Verify mean-normalized patch residual at converged offset.
        var sum_d: f32 = 0.0;
        for (var oy: i32 = -r; oy <= r; oy = oy + 1) {
            for (var ox: i32 = -r; ox <= r; ox = ox + 1) {
                let tx = px + f32(ox);
                let ty = py + f32(oy);
                let s_tpl = fetch_tpl(meta, tx, ty, forward);
                let s_tgt = fetch_tgt(meta, tx + gx, ty + gy, forward);
                if s_tgt.w < 0.0 {
                    return vec3<f32>(0.0, 0.0, -1.0);
                }
                sum_d = sum_d + (s_tgt.x - s_tpl.x);
            }
        }
        let mean_d = sum_d * inv_n;
        var mad: f32 = 0.0;
        for (var oy: i32 = -r; oy <= r; oy = oy + 1) {
            for (var ox: i32 = -r; ox <= r; ox = ox + 1) {
                let tx = px + f32(ox);
                let ty = py + f32(oy);
                let s_tpl = fetch_tpl(meta, tx, ty, forward);
                let s_tgt = fetch_tgt(meta, tx + gx, ty + gy, forward);
                mad = mad + abs((s_tgt.x - s_tpl.x) - mean_d);
            }
        }
        if mad * inv_n > 0.22 {
            return vec3<f32>(0.0, 0.0, -1.0);
        }
        let nx = px + gx;
        let ny = py + gy;
        point = vec2<f32>(nx * s, ny * s);
        guess = vec2<f32>((nx - px) * 2.0, (ny - py) * 2.0);
    }
    return vec3<f32>(point.x, point.y, 1.0);
}

@compute @workgroup_size(64, 1, 1)
fn cs_pyrlk_fb(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= ulk.count {
        return;
    }
    let q = queries[i];
    let p = q.xy;
    let guess = q.zw;
    let r = max(ulk.patch_r, 2);

    let fwd = track_one_dir(p, guess, r, true);
    if fwd.z < 0.0 {
        results[i] = vec4<f32>(0.0, 0.0, -1.0, 999.0);
        return;
    }
    let back_guess = p - fwd.xy;
    let bwd = track_one_dir(fwd.xy, back_guess, r, false);
    if bwd.z < 0.0 {
        results[i] = vec4<f32>(0.0, 0.0, -1.0, 999.0);
        return;
    }
    let fb_err = length(bwd.xy - p);
    if fb_err > ulk.fb_max_px {
        results[i] = vec4<f32>(0.0, 0.0, -1.0, fb_err);
        return;
    }
    results[i] = vec4<f32>(fwd.x, fwd.y, 1.0, fb_err);
}

@compute @workgroup_size(64, 1, 1)
fn cs_zncc_search(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    let span = u32(2 * ulk.search_range + 1);
    let total = span * span;
    if idx >= total || ulk.levels == 0u {
        return;
    }
    let top_l = ulk.levels - 1u;
    let meta = level_meta(top_l);
    let s = f32(1u << top_l);
    let cx = ulk.pt_x / s;
    let cy = ulk.pt_y / s;
    let r = clamp(ulk.patch_r, 2, 4);
    let rf = f32(r);
    let dx = i32(idx % span) - ulk.search_range;
    let dy = i32(idx / span) - ulk.search_range;
    let tx = cx + f32(dx);
    let ty = cy + f32(dy);

    if cx - rf < 0.0 || cy - rf < 0.0 || cx + rf >= f32(meta.y) || cy + rf >= f32(meta.z)
        || tx - rf < 0.0 || ty - rf < 0.0 || tx + rf >= f32(meta.y) || ty + rf >= f32(meta.z)
    {
        results[idx] = vec4<f32>(-1.0, 0.0, 0.0, 0.0);
        return;
    }

    let inv_n = 1.0 / f32((2 * r + 1) * (2 * r + 1));
    var sum_a: f32 = 0.0;
    var sum_b: f32 = 0.0;
    for (var oy: i32 = -r; oy <= r; oy = oy + 1) {
        for (var ox: i32 = -r; ox <= r; ox = ox + 1) {
            sum_a = sum_a + sample_prev(meta, cx + f32(ox), cy + f32(oy)).x;
            sum_b = sum_b + sample_curr(meta, tx + f32(ox), ty + f32(oy)).x;
        }
    }
    let mean_a = sum_a * inv_n;
    let mean_b = sum_b * inv_n;
    var dot_ab: f32 = 0.0;
    var na: f32 = 0.0;
    var nb: f32 = 0.0;
    for (var oy: i32 = -r; oy <= r; oy = oy + 1) {
        for (var ox: i32 = -r; ox <= r; ox = ox + 1) {
            let va = sample_prev(meta, cx + f32(ox), cy + f32(oy)).x - mean_a;
            let vb = sample_curr(meta, tx + f32(ox), ty + f32(oy)).x - mean_b;
            dot_ab = dot_ab + va * vb;
            na = na + va * va;
            nb = nb + vb * vb;
        }
    }
    let denom = sqrt(na * nb);
    if denom < 1e-6 {
        results[idx] = vec4<f32>(-1.0, 0.0, 0.0, 0.0);
        return;
    }
    results[idx] = vec4<f32>(dot_ab / denom, f32(dx) * s, f32(dy) * s, 1.0);
}

fn stereo_mse(meta: vec4<u32>, pt: vec2<f32>, r: i32, disp: f32) -> vec2<f32> {
    var sum: f32 = 0.0;
    var n: u32 = 0u;
    for (var dy: i32 = -r; dy <= r; dy = dy + 1) {
        for (var dx: i32 = -r; dx <= r; dx = dx + 1) {
            let x = pt.x + f32(dx);
            let y = pt.y + f32(dy);
            let la = sample_prev(meta, x, y);
            let rb = sample_curr(meta, x - disp, y);
            if la.w > 0.0 && rb.w > 0.0 {
                let e = la.x - rb.x;
                sum = sum + e * e;
                n = n + 1u;
            }
        }
    }
    if n == 0u {
        return vec2<f32>(1e9, -1.0);
    }
    return vec2<f32>(sum / f32(n), 1.0);
}

@compute @workgroup_size(64, 1, 1)
fn cs_stereo_disp(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if i >= ulk.count {
        return;
    }
    let meta = ulk.lvl0;
    let pt = queries[i].xy;
    let r = clamp(ulk.patch_r, 3, 9);
    let max_d = max(ulk.max_disp, 1.0);
    let steps = u32(floor(max_d));

    var best_d: f32 = 0.0;
    var best_s: f32 = 1e9;
    var found = false;
    for (var di: u32 = 0u; di <= steps; di = di + 1u) {
        let d = f32(di);
        let sc = stereo_mse(meta, pt, r, d);
        if sc.y > 0.0 && sc.x < best_s {
            best_s = sc.x;
            best_d = d;
            found = true;
        }
    }
    if !found {
        results[i] = vec4<f32>(0.0, -1.0, 0.0, 0.0);
        return;
    }
    let sl = stereo_mse(meta, pt, r, max(best_d - 1.0, 0.0));
    let sc = stereo_mse(meta, pt, r, best_d);
    let sr = stereo_mse(meta, pt, r, min(best_d + 1.0, max_d));
    var refined = best_d;
    let curvature = sl.x + sr.x - 2.0 * sc.x;
    if sl.y > 0.0 && sc.y > 0.0 && sr.y > 0.0 && curvature > 1e-9 {
        refined = best_d + clamp(0.5 * (sl.x - sr.x) / curvature, -0.5, 0.5);
    }
    results[i] = vec4<f32>(clamp(refined / max_d, 0.0, 1.0), 1.0, best_s, 0.0);
}
