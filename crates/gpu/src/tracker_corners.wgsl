// Module 2 of 3 for GPU motion tracking: tiled Shi-Tomasi corner detector.

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
    // Accept natural low/medium-contrast real-video corners while rejecting blank uniform regions.
    if best_eig > 8e-5 {
        out_corners[tid] = vec4<f32>(best_eig, best_x, best_y, 1.0);
    } else {
        out_corners[tid] = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
}
