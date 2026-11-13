// Standalone Naga 30 shader validation without a Rust/JDK toolchain.
// Install naga-wasm@30.2.0 outside the checkout, then set NAGA_MODULE to its dist/node.js URL.
import fs from 'node:fs';
const { parseWgsl, validate, writeSpirv, writeGlsl, nagaVersion } = await import(process.env.NAGA_MODULE ?? 'naga-wasm');
if (!String(nagaVersion).startsWith('30.')) throw new Error(`Expected Naga 30, got ${nagaVersion}`);
const root = new URL('../crates/gpu/src/', import.meta.url);
const stroke = fs.readFileSync(new URL('drawing.wgsl', root), 'utf8');
const dabs = fs.readFileSync(new URL('drawing_dabs.wgsl', root), 'utf8');
// Must match Painter::new's downlevel shader specialization.
const compatible = dabs.replaceAll('@interpolate(perspective, sample)', '@interpolate(perspective, center)')
  .replaceAll(', @builtin(sample_index) sample: u32', '')
  .replaceAll('if sample >= 4u { discard; }', '');
console.log(`Naga ${nagaVersion}`);
for (const [name, source] of [['stroke', stroke], ['sample-rate dabs', dabs], ['per-fragment dabs', compatible]]) {
  const module = parseWgsl(source);
  const info = validate(module);
  const spirv = writeSpirv(module, info);
  console.log(`${name}: parsed, validated, SPIR-V emitted (${spirv.length} words)`);
}
const module = parseWgsl(compatible);
const info = validate(module);
for (const [stage, entryPoint] of [['vertex', 'vs_dab'], ['fragment', 'fs_dab']]) {
  const glsl = writeGlsl(module, info, { version: '300 es', stage, entryPoint });
  if (!glsl.includes('#version 300 es')) throw new Error('Wrong GLSL target');
  console.log(`GLES 3.0 ${stage}: translated (${glsl.length} characters)`);
}
