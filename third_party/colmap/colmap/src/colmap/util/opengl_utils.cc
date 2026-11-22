// SPDX-License-Identifier: BSD-3-Clause
#include "colmap/util/opengl_utils.h"

#include "colmap/util/logging.h"

#include <Eigen/LU>

#include <algorithm>
#include <cmath>
#include <cstring>
#include <mutex>

#if defined(__unix__) || defined(__ANDROID__) || defined(__linux__)
#include <dlfcn.h>
#define COLMAP_HAS_POSIX_DL 1
#else
#define COLMAP_HAS_POSIX_DL 0
#endif

namespace colmap {
namespace {

using EGLDisplay = void*;
using EGLConfig = void*;
using EGLContext = void*;
using EGLSurface = void*;
using EGLNativeDisplayType = void*;
using EGLint = int32_t;
using EGLBoolean = unsigned int;
using EGLenum = unsigned int;

using GLenum = unsigned int;
using GLuint = unsigned int;
using GLint = int;
using GLsizei = int;
using GLboolean = unsigned char;
using GLbitfield = unsigned int;
using GLchar = char;
using GLsizeiptr = ptrdiff_t;
using GLintptr = ptrdiff_t;

constexpr EGLint kEglNone = 0x3038;
constexpr EGLint kEglSurfaceType = 0x3033;
constexpr EGLint kEglPbufferBit = 0x0001;
constexpr EGLint kEglRenderableType = 0x3040;
constexpr EGLint kEglOpenGLES3Bit = 0x00000040;
constexpr EGLint kEglRedSize = 0x3024;
constexpr EGLint kEglGreenSize = 0x3023;
constexpr EGLint kEglBlueSize = 0x3022;
constexpr EGLint kEglWidth = 0x3057;
constexpr EGLint kEglHeight = 0x3056;
constexpr EGLint kEglContextClientVersion = 0x3098;
constexpr EGLint kEglContextMinorVersion = 0x30FB;
constexpr EGLenum kEglOpenGLESAPI = 0x30A0;

constexpr GLenum kGlComputeShader = 0x91B9;
constexpr GLenum kGlCompileStatus = 0x8B81;
constexpr GLenum kGlLinkStatus = 0x8B82;
constexpr GLenum kGlShaderStorageBuffer = 0x90D2;
constexpr GLenum kGlDynamicDraw = 0x88E8;
constexpr GLbitfield kGlShaderStorageBarrierBit = 0x00002000;
constexpr GLbitfield kGlBufferUpdateBarrierBit = 0x00000100;
constexpr GLbitfield kGlMapReadBit = 0x0001;

struct EglGlApi {
  void* egl_lib = nullptr;
  void* gl_lib = nullptr;
  bool loaded = false;

  EGLDisplay (*eglGetDisplay)(EGLNativeDisplayType) = nullptr;
  EGLBoolean (*eglInitialize)(EGLDisplay, EGLint*, EGLint*) = nullptr;
  EGLBoolean (*eglBindAPI)(EGLenum) = nullptr;
  EGLBoolean (*eglChooseConfig)(EGLDisplay, const EGLint*, EGLConfig*, EGLint, EGLint*) = nullptr;
  EGLSurface (*eglCreatePbufferSurface)(EGLDisplay, EGLConfig, const EGLint*) = nullptr;
  EGLContext (*eglCreateContext)(EGLDisplay, EGLConfig, EGLContext, const EGLint*) = nullptr;
  EGLBoolean (*eglMakeCurrent)(EGLDisplay, EGLSurface, EGLSurface, EGLContext) = nullptr;
  EGLContext (*eglGetCurrentContext)() = nullptr;
  EGLBoolean (*eglDestroySurface)(EGLDisplay, EGLSurface) = nullptr;
  EGLBoolean (*eglDestroyContext)(EGLDisplay, EGLContext) = nullptr;
  EGLBoolean (*eglTerminate)(EGLDisplay) = nullptr;
  void* (*eglGetProcAddress)(const char*) = nullptr;

  GLuint (*glCreateShader)(GLenum) = nullptr;
  void (*glShaderSource)(GLuint, GLsizei, const GLchar* const*, const GLint*) = nullptr;
  void (*glCompileShader)(GLuint) = nullptr;
  void (*glGetShaderiv)(GLuint, GLenum, GLint*) = nullptr;
  void (*glDeleteShader)(GLuint) = nullptr;
  GLuint (*glCreateProgram)() = nullptr;
  void (*glAttachShader)(GLuint, GLuint) = nullptr;
  void (*glLinkProgram)(GLuint) = nullptr;
  void (*glGetProgramiv)(GLuint, GLenum, GLint*) = nullptr;
  void (*glUseProgram)(GLuint) = nullptr;
  void (*glDeleteProgram)(GLuint) = nullptr;
  GLint (*glGetUniformLocation)(GLuint, const GLchar*) = nullptr;
  void (*glUniform1i)(GLint, GLint) = nullptr;
  void (*glUniform1f)(GLint, float) = nullptr;
  void (*glGenBuffers)(GLsizei, GLuint*) = nullptr;
  void (*glDeleteBuffers)(GLsizei, const GLuint*) = nullptr;
  void (*glBindBuffer)(GLenum, GLuint) = nullptr;
  void (*glBindBufferBase)(GLenum, GLuint, GLuint) = nullptr;
  void (*glBufferData)(GLenum, GLsizeiptr, const void*, GLenum) = nullptr;
  void* (*glMapBufferRange)(GLenum, GLintptr, GLsizeiptr, GLbitfield) = nullptr;
  GLboolean (*glUnmapBuffer)(GLenum) = nullptr;
  void (*glDispatchCompute)(GLuint, GLuint, GLuint) = nullptr;
  void (*glMemoryBarrier)(GLbitfield) = nullptr;
  void (*glFinish)() = nullptr;

  bool Load() {
#if COLMAP_HAS_POSIX_DL
    if (loaded) return true;
    const char* egl_names[] = {"libEGL.so", "libEGL.so.1"};
    for (const char* n : egl_names) {
      egl_lib = dlopen(n, RTLD_NOW | RTLD_LOCAL);
      if (egl_lib) break;
    }
    const char* gl_names[] = {"libGLESv3.so", "libGLESv2.so", "libGLESv2.so.2"};
    for (const char* n : gl_names) {
      gl_lib = dlopen(n, RTLD_NOW | RTLD_LOCAL);
      if (gl_lib) break;
    }
    if (!egl_lib || !gl_lib) return false;

    auto load_egl = [&](const char* sym) -> void* { return dlsym(egl_lib, sym); };
    eglGetDisplay = reinterpret_cast<decltype(eglGetDisplay)>(load_egl("eglGetDisplay"));
    eglInitialize = reinterpret_cast<decltype(eglInitialize)>(load_egl("eglInitialize"));
    eglBindAPI = reinterpret_cast<decltype(eglBindAPI)>(load_egl("eglBindAPI"));
    eglChooseConfig = reinterpret_cast<decltype(eglChooseConfig)>(load_egl("eglChooseConfig"));
    eglCreatePbufferSurface = reinterpret_cast<decltype(eglCreatePbufferSurface)>(load_egl("eglCreatePbufferSurface"));
    eglCreateContext = reinterpret_cast<decltype(eglCreateContext)>(load_egl("eglCreateContext"));
    eglMakeCurrent = reinterpret_cast<decltype(eglMakeCurrent)>(load_egl("eglMakeCurrent"));
    eglGetCurrentContext = reinterpret_cast<decltype(eglGetCurrentContext)>(load_egl("eglGetCurrentContext"));
    eglDestroySurface = reinterpret_cast<decltype(eglDestroySurface)>(load_egl("eglDestroySurface"));
    eglDestroyContext = reinterpret_cast<decltype(eglDestroyContext)>(load_egl("eglDestroyContext"));
    eglTerminate = reinterpret_cast<decltype(eglTerminate)>(load_egl("eglTerminate"));
    eglGetProcAddress = reinterpret_cast<decltype(eglGetProcAddress)>(load_egl("eglGetProcAddress"));

    if (!eglGetDisplay || !eglInitialize || !eglChooseConfig ||
        !eglCreatePbufferSurface || !eglCreateContext || !eglMakeCurrent) {
      return false;
    }

    auto load_gl = [&](const char* sym) -> void* {
      void* p = dlsym(gl_lib, sym);
      if (!p && eglGetProcAddress) p = eglGetProcAddress(sym);
      return p;
    };
    glCreateShader = reinterpret_cast<decltype(glCreateShader)>(load_gl("glCreateShader"));
    glShaderSource = reinterpret_cast<decltype(glShaderSource)>(load_gl("glShaderSource"));
    glCompileShader = reinterpret_cast<decltype(glCompileShader)>(load_gl("glCompileShader"));
    glGetShaderiv = reinterpret_cast<decltype(glGetShaderiv)>(load_gl("glGetShaderiv"));
    glDeleteShader = reinterpret_cast<decltype(glDeleteShader)>(load_gl("glDeleteShader"));
    glCreateProgram = reinterpret_cast<decltype(glCreateProgram)>(load_gl("glCreateProgram"));
    glAttachShader = reinterpret_cast<decltype(glAttachShader)>(load_gl("glAttachShader"));
    glLinkProgram = reinterpret_cast<decltype(glLinkProgram)>(load_gl("glLinkProgram"));
    glGetProgramiv = reinterpret_cast<decltype(glGetProgramiv)>(load_gl("glGetProgramiv"));
    glUseProgram = reinterpret_cast<decltype(glUseProgram)>(load_gl("glUseProgram"));
    glDeleteProgram = reinterpret_cast<decltype(glDeleteProgram)>(load_gl("glDeleteProgram"));
    glGetUniformLocation = reinterpret_cast<decltype(glGetUniformLocation)>(load_gl("glGetUniformLocation"));
    glUniform1i = reinterpret_cast<decltype(glUniform1i)>(load_gl("glUniform1i"));
    glUniform1f = reinterpret_cast<decltype(glUniform1f)>(load_gl("glUniform1f"));
    glGenBuffers = reinterpret_cast<decltype(glGenBuffers)>(load_gl("glGenBuffers"));
    glDeleteBuffers = reinterpret_cast<decltype(glDeleteBuffers)>(load_gl("glDeleteBuffers"));
    glBindBuffer = reinterpret_cast<decltype(glBindBuffer)>(load_gl("glBindBuffer"));
    glBindBufferBase = reinterpret_cast<decltype(glBindBufferBase)>(load_gl("glBindBufferBase"));
    glBufferData = reinterpret_cast<decltype(glBufferData)>(load_gl("glBufferData"));
    glMapBufferRange = reinterpret_cast<decltype(glMapBufferRange)>(load_gl("glMapBufferRange"));
    glUnmapBuffer = reinterpret_cast<decltype(glUnmapBuffer)>(load_gl("glUnmapBuffer"));
    glDispatchCompute = reinterpret_cast<decltype(glDispatchCompute)>(load_gl("glDispatchCompute"));
    glMemoryBarrier = reinterpret_cast<decltype(glMemoryBarrier)>(load_gl("glMemoryBarrier"));
    glFinish = reinterpret_cast<decltype(glFinish)>(load_gl("glFinish"));

    loaded = glCreateShader && glShaderSource && glCompileShader &&
             glCreateProgram && glLinkProgram && glUseProgram &&
             glGenBuffers && glBindBufferBase && glBufferData &&
             glMapBufferRange && glUnmapBuffer && glDispatchCompute &&
             glMemoryBarrier;
    return loaded;
#else
    return false;
#endif
  }
};

EglGlApi& GetGlApi() {
  static EglGlApi api;
  static std::once_flag once;
  std::call_once(once, [&]() { api.Load(); });
  return api;
}

GLuint CompileComputeProgram(EglGlApi& gl, const char* src) {
  GLuint sh = gl.glCreateShader(kGlComputeShader);
  if (!sh) return 0;
  gl.glShaderSource(sh, 1, &src, nullptr);
  gl.glCompileShader(sh);
  GLint ok = 0;
  gl.glGetShaderiv(sh, kGlCompileStatus, &ok);
  if (!ok) {
    gl.glDeleteShader(sh);
    return 0;
  }
  GLuint prog = gl.glCreateProgram();
  gl.glAttachShader(prog, sh);
  gl.glLinkProgram(prog);
  gl.glDeleteShader(sh);
  gl.glGetProgramiv(prog, kGlLinkStatus, &ok);
  if (!ok) {
    gl.glDeleteProgram(prog);
    return 0;
  }
  return prog;
}

// OpenGL ES 3.1 compute shader: separable horizontal/vertical Gaussian blur.
const char* kBlurShaderSrc = R"(#version 310 es
precision highp float;
layout(local_size_x = 16, local_size_y = 16) in;
layout(std430, binding = 0) readonly buffer SrcBuf { float src_img[]; };
layout(std430, binding = 1) writeonly buffer DstBuf { float dst_img[]; };
layout(std430, binding = 2) readonly buffer KernelBuf { float weights[]; };
uniform int u_width;
uniform int u_height;
uniform int u_radius;
uniform int u_horizontal;
void main() {
  int x = int(gl_GlobalInvocationID.x);
  int y = int(gl_GlobalInvocationID.y);
  if (x >= u_width || y >= u_height) return;
  float acc = 0.0;
  if (u_horizontal != 0) {
    for (int k = -u_radius; k <= u_radius; ++k) {
      int sx = clamp(x + k, 0, u_width - 1);
      acc += src_img[y * u_width + sx] * weights[k + u_radius];
    }
  } else {
    for (int k = -u_radius; k <= u_radius; ++k) {
      int sy = clamp(y + k, 0, u_height - 1);
      acc += src_img[sy * u_width + x] * weights[k + u_radius];
    }
  }
  dst_img[y * u_width + x] = acc;
}
)";

// OpenGL ES 3.1 compute shader: L2 SIFT descriptor nearest + second-nearest neighbor search.
const char* kMatchShaderSrc = R"(#version 310 es
precision highp float;
layout(local_size_x = 64) in;
// Packed 4 uint8 descriptors per uint32 => 32 uints per 128D SIFT descriptor.
layout(std430, binding = 0) readonly buffer Desc1Buf { uint d1[]; };
layout(std430, binding = 1) readonly buffer Desc2Buf { uint d2[]; };
layout(std430, binding = 2) writeonly buffer OutBuf { ivec2 out_best[]; };
layout(std430, binding = 3) writeonly buffer DistBuf { vec2 out_dots[]; };
uniform int u_n1;
uniform int u_n2;
int dot4u8(uint a, uint b) {
  int s = int(a & 255u) * int(b & 255u);
  s += int((a >> 8) & 255u) * int((b >> 8) & 255u);
  s += int((a >> 16) & 255u) * int((b >> 16) & 255u);
  s += int((a >> 24) & 255u) * int((b >> 24) & 255u);
  return s;
}
void main() {
  int i1 = int(gl_GlobalInvocationID.x);
  if (i1 >= u_n1) return;
  uint v1[32];
  int base1 = i1 * 32;
  for (int k = 0; k < 32; ++k) v1[k] = d1[base1 + k];
  int best_idx = -1;
  int second_idx = -1;
  int best_dot = 0;
  int second_dot = 0;
  for (int i2 = 0; i2 < u_n2; ++i2) {
    int base2 = i2 * 32;
    int dotv = 0;
    for (int k = 0; k < 32; ++k) {
      dotv += dot4u8(v1[k], d2[base2 + k]);
    }
    if (dotv > best_dot) {
      second_dot = best_dot;
      second_idx = best_idx;
      best_dot = dotv;
      best_idx = i2;
    } else if (dotv > second_dot) {
      second_dot = dotv;
      second_idx = i2;
    }
  }
  out_best[i1] = ivec2(best_idx, second_idx);
  out_dots[i1] = vec2(float(best_dot), float(second_dot));
}
)";

}  // namespace

struct OpenGLContextManager::Impl {
  int major = 3;
  int minor = 1;
  EGLDisplay display = nullptr;
  EGLSurface surface = nullptr;
  EGLContext context = nullptr;
  bool valid = false;

  Impl(int maj, int min_v) : major(maj), minor(min_v) {
    auto& api = GetGlApi();
    if (!api.loaded) return;
    display = api.eglGetDisplay(nullptr);
    if (!display) return;
    EGLint egl_maj = 0, egl_min = 0;
    if (!api.eglInitialize(display, &egl_maj, &egl_min)) {
      display = nullptr;
      return;
    }
    if (api.eglBindAPI) api.eglBindAPI(kEglOpenGLESAPI);
    const EGLint cfg_attribs[] = {
        kEglSurfaceType, kEglPbufferBit,
        kEglRenderableType, kEglOpenGLES3Bit,
        kEglRedSize, 8,
        kEglGreenSize, 8,
        kEglBlueSize, 8,
        kEglNone};
    EGLConfig cfg = nullptr;
    EGLint num_cfg = 0;
    if (!api.eglChooseConfig(display, cfg_attribs, &cfg, 1, &num_cfg) || num_cfg < 1) {
      return;
    }
    const EGLint pbuf_attribs[] = {kEglWidth, 16, kEglHeight, 16, kEglNone};
    surface = api.eglCreatePbufferSurface(display, cfg, pbuf_attribs);
    if (!surface) return;
    const EGLint ctx_attribs31[] = {
        kEglContextClientVersion, 3,
        kEglContextMinorVersion, 1,
        kEglNone};
    context = api.eglCreateContext(display, cfg, nullptr, ctx_attribs31);
    if (!context) {
      const EGLint ctx_attribs30[] = {kEglContextClientVersion, 3, kEglNone};
      context = api.eglCreateContext(display, cfg, nullptr, ctx_attribs30);
    }
    if (!context) return;
    valid = api.eglMakeCurrent(display, surface, surface, context) != 0;
  }

  ~Impl() {
    auto& api = GetGlApi();
    if (!api.loaded || !display) return;
    api.eglMakeCurrent(display, nullptr, nullptr, nullptr);
    if (context && api.eglDestroyContext) api.eglDestroyContext(display, context);
    if (surface && api.eglDestroySurface) api.eglDestroySurface(display, surface);
  }
};

OpenGLContextManager::OpenGLContextManager(int opengl_major_version,
                                           int opengl_minor_version)
    : impl_(std::make_unique<Impl>(opengl_major_version, opengl_minor_version)) {}

OpenGLContextManager::~OpenGLContextManager() = default;

bool OpenGLContextManager::MakeCurrent() {
  if (!impl_ || !impl_->display || !impl_->context || !impl_->surface) return false;
  auto& api = GetGlApi();
  impl_->valid = api.eglMakeCurrent(impl_->display, impl_->surface, impl_->surface, impl_->context) != 0;
  return impl_->valid;
}

bool OpenGLContextManager::IsValid() const {
  return impl_ && impl_->valid;
}

void RunThreadWithOpenGLContext(Thread* thread) {
  thread->Start();
  thread->Wait();
}

struct OpenGLSiftEngine::Impl {
  GLuint blur_prog = 0;
  GLuint match_prog = 0;
  GLuint bufs[4] = {0, 0, 0, 0};
  bool ready = false;

  ~Impl() {
    if (!ready) return;
    auto& gl = GetGlApi();
    if (!gl.loaded || !gl.eglGetCurrentContext || !gl.eglGetCurrentContext()) return;
    if (bufs[0]) gl.glDeleteBuffers(4, bufs);
    if (blur_prog) gl.glDeleteProgram(blur_prog);
    if (match_prog) gl.glDeleteProgram(match_prog);
  }
};

OpenGLSiftEngine::OpenGLSiftEngine() : impl_(std::make_unique<Impl>()) {}
OpenGLSiftEngine::~OpenGLSiftEngine() = default;

bool OpenGLSiftEngine::Initialize() {
  if (impl_->ready) return true;
  auto& gl = GetGlApi();
  if (!gl.loaded || !gl.eglGetCurrentContext || !gl.eglGetCurrentContext()) {
    return false;
  }
  impl_->blur_prog = CompileComputeProgram(gl, kBlurShaderSrc);
  impl_->match_prog = CompileComputeProgram(gl, kMatchShaderSrc);
  if (!impl_->blur_prog || !impl_->match_prog) {
    if (impl_->blur_prog) gl.glDeleteProgram(impl_->blur_prog);
    if (impl_->match_prog) gl.glDeleteProgram(impl_->match_prog);
    impl_->blur_prog = 0;
    impl_->match_prog = 0;
    return false;
  }
  gl.glGenBuffers(4, impl_->bufs);
  impl_->ready = (impl_->bufs[0] != 0);
  return impl_->ready;
}

bool OpenGLSiftEngine::IsAvailable() const { return impl_ && impl_->ready; }

bool OpenGLSiftEngine::Extract(const uint8_t* gray,
                               int width,
                               int height,
                               int max_features,
                               int num_octaves,
                               int octave_resolution,
                               float peak_threshold,
                               float edge_threshold,
                               bool l1_root_norm,
                               FeatureKeypoints* keypoints,
                               FeatureDescriptorsData* descriptors) {
  if (!IsAvailable() || width < 16 || height < 16) return false;
  auto& gl = GetGlApi();

  auto gpu_gaussian_blur = [&](const std::vector<float>& src,
                               int w,
                               int h,
                               float sigma,
                               std::vector<float>* dst) -> bool {
    const int radius = std::clamp(static_cast<int>(std::ceil(3.0f * sigma)), 1, 24);
    std::vector<float> kernel(2 * radius + 1);
    float ksum = 0.0f;
    for (int k = -radius; k <= radius; ++k) {
      const float v = std::exp(-0.5f * (k * k) / (sigma * sigma));
      kernel[k + radius] = v;
      ksum += v;
    }
    for (float& v : kernel) v /= ksum;

    const GLsizeiptr img_bytes = static_cast<GLsizeiptr>(w) * h * sizeof(float);
    gl.glBindBuffer(kGlShaderStorageBuffer, impl_->bufs[0]);
    gl.glBufferData(kGlShaderStorageBuffer, img_bytes, src.data(), kGlDynamicDraw);
    gl.glBindBuffer(kGlShaderStorageBuffer, impl_->bufs[1]);
    gl.glBufferData(kGlShaderStorageBuffer, img_bytes, nullptr, kGlDynamicDraw);
    gl.glBindBuffer(kGlShaderStorageBuffer, impl_->bufs[2]);
    gl.glBufferData(kGlShaderStorageBuffer,
                    static_cast<GLsizeiptr>(kernel.size() * sizeof(float)),
                    kernel.data(),
                    kGlDynamicDraw);

    gl.glUseProgram(impl_->blur_prog);
    gl.glUniform1i(gl.glGetUniformLocation(impl_->blur_prog, "u_width"), w);
    gl.glUniform1i(gl.glGetUniformLocation(impl_->blur_prog, "u_height"), h);
    gl.glUniform1i(gl.glGetUniformLocation(impl_->blur_prog, "u_radius"), radius);

    const GLuint gx = static_cast<GLuint>((w + 15) / 16);
    const GLuint gy = static_cast<GLuint>((h + 15) / 16);

    // Horizontal pass: bufs[0] -> bufs[1]
    gl.glBindBufferBase(kGlShaderStorageBuffer, 0, impl_->bufs[0]);
    gl.glBindBufferBase(kGlShaderStorageBuffer, 1, impl_->bufs[1]);
    gl.glBindBufferBase(kGlShaderStorageBuffer, 2, impl_->bufs[2]);
    gl.glUniform1i(gl.glGetUniformLocation(impl_->blur_prog, "u_horizontal"), 1);
    gl.glDispatchCompute(gx, gy, 1);
    gl.glMemoryBarrier(kGlShaderStorageBarrierBit);

    // Vertical pass: bufs[1] -> bufs[0]
    gl.glBindBufferBase(kGlShaderStorageBuffer, 0, impl_->bufs[1]);
    gl.glBindBufferBase(kGlShaderStorageBuffer, 1, impl_->bufs[0]);
    gl.glUniform1i(gl.glGetUniformLocation(impl_->blur_prog, "u_horizontal"), 0);
    gl.glDispatchCompute(gx, gy, 1);
    gl.glMemoryBarrier(kGlShaderStorageBarrierBit | kGlBufferUpdateBarrierBit);

    gl.glBindBuffer(kGlShaderStorageBuffer, impl_->bufs[0]);
    const float* mapped = static_cast<const float*>(
        gl.glMapBufferRange(kGlShaderStorageBuffer, 0, img_bytes, kGlMapReadBit));
    if (!mapped) return false;
    dst->assign(mapped, mapped + static_cast<size_t>(w) * h);
    gl.glUnmapBuffer(kGlShaderStorageBuffer);
    return true;
  };

  // Build OpenGL Gaussian scale-space & DoG pyramid, extract subpixel SIFT keypoints + 128D descriptors.
  const int S = std::clamp(octave_resolution, 2, 5);
  const int n_gauss = S + 3;
  const int octaves = std::clamp(num_octaves, 1, 5);
  const float sigma0 = 1.6f;
  const float k_step = std::pow(2.0f, 1.0f / S);

  std::vector<float> base(static_cast<size_t>(width) * height);
  for (size_t i = 0; i < base.size(); ++i) {
    base[i] = static_cast<float>(gray[i]) * (1.0f / 255.0f);
  }

  struct RawKp {
    float x, y, scale, angle, response;
    int octave, level;
    std::array<float, 128> desc;
  };
  std::vector<RawKp> candidates;
  candidates.reserve(static_cast<size_t>(max_features) * 2);

  int ow = width;
  int oh = height;
  std::vector<float> oct_base = std::move(base);
  const float edge_ratio = ((edge_threshold + 1.0f) * (edge_threshold + 1.0f)) / edge_threshold;
  const float vl_peak_thr = peak_threshold / static_cast<float>(S);
  const float pre_thr = 0.8f * vl_peak_thr;

  for (int o = 0; o < octaves && ow >= 24 && oh >= 24; ++o) {
    std::vector<std::vector<float>> gauss(n_gauss);
    if (o == 0) {
      const float init_sig = std::sqrt(std::max(0.25f, sigma0 * sigma0 - 0.25f));
      if (!gpu_gaussian_blur(oct_base, ow, oh, init_sig, &gauss[0])) return false;
    } else {
      gauss[0] = std::move(oct_base);
    }
    for (int s = 1; s < n_gauss; ++s) {
      const float sig_prev = sigma0 * std::pow(k_step, s - 1);
      const float sig_curr = sig_prev * k_step;
      const float dsig = std::sqrt(std::max(0.04f, sig_curr * sig_curr - sig_prev * sig_prev));
      if (!gpu_gaussian_blur(gauss[s - 1], ow, oh, dsig, &gauss[s])) return false;
    }

    std::vector<std::vector<float>> dog(n_gauss - 1, std::vector<float>(static_cast<size_t>(ow) * oh));
    for (int s = 0; s < n_gauss - 1; ++s) {
      for (size_t i = 0; i < dog[s].size(); ++i) {
        dog[s][i] = gauss[s + 1][i] - gauss[s][i];
      }
    }

    const float oct_scale = static_cast<float>(1 << o);
    for (int s = 1; s <= S; ++s) {
      const auto& dm = dog[s - 1];
      const auto& dc = dog[s];
      const auto& dp = dog[s + 1];

      for (int y = 4; y < oh - 4; ++y) {
        for (int x = 4; x < ow - 4; ++x) {
          const float v = dc[y * ow + x];
          if (std::fabs(v) < pre_thr) continue;
          bool is_max = v > 0.0f;
          bool is_min = v < 0.0f;
          for (int dz = -1; dz <= 1 && (is_max || is_min); ++dz) {
            const auto& dl = (dz < 0) ? dm : (dz > 0 ? dp : dc);
            for (int dy = -1; dy <= 1; ++dy) {
              for (int dx = -1; dx <= 1; ++dx) {
                if (dx == 0 && dy == 0 && dz == 0) continue;
                const float nv = dl[(y + dy) * ow + (x + dx)];
                if (nv >= v) is_max = false;
                if (nv <= v) is_min = false;
              }
            }
          }
          if (!is_max && !is_min) continue;

          // 3D (x, y, s) subpixel refinement + Hessian edge rejection (Lowe 2004 / VLFeat)
          int rx_i = x;
          int ry_i = y;
          int rs_i = s;
          float ox = 0.0f, oy = 0.0f, os = 0.0f;
          bool converged = false;
          for (int iter = 0; iter < 5; ++iter) {
            const auto& d_m = dog[rs_i - 1];
            const auto& d_c = dog[rs_i];
            const auto& d_p = dog[rs_i + 1];
            const float vc = d_c[ry_i * ow + rx_i];
            const float gx = 0.5f * (d_c[ry_i * ow + rx_i + 1] - d_c[ry_i * ow + rx_i - 1]);
            const float gy = 0.5f * (d_c[(ry_i + 1) * ow + rx_i] - d_c[(ry_i - 1) * ow + rx_i]);
            const float gs = 0.5f * (d_p[ry_i * ow + rx_i] - d_m[ry_i * ow + rx_i]);
            const float hxx = d_c[ry_i * ow + rx_i + 1] + d_c[ry_i * ow + rx_i - 1] - 2.0f * vc;
            const float hyy = d_c[(ry_i + 1) * ow + rx_i] + d_c[(ry_i - 1) * ow + rx_i] - 2.0f * vc;
            const float hss = d_p[ry_i * ow + rx_i] + d_m[ry_i * ow + rx_i] - 2.0f * vc;
            const float hxy = 0.25f * (d_c[(ry_i + 1) * ow + rx_i + 1] - d_c[(ry_i + 1) * ow + rx_i - 1] -
                                       d_c[(ry_i - 1) * ow + rx_i + 1] + d_c[(ry_i - 1) * ow + rx_i - 1]);
            const float hxs = 0.25f * (d_p[ry_i * ow + rx_i + 1] - d_p[ry_i * ow + rx_i - 1] -
                                       d_m[ry_i * ow + rx_i + 1] + d_m[ry_i * ow + rx_i - 1]);
            const float hys = 0.25f * (d_p[(ry_i + 1) * ow + rx_i] - d_p[(ry_i - 1) * ow + rx_i] -
                                       d_m[(ry_i + 1) * ow + rx_i] + d_m[(ry_i - 1) * ow + rx_i]);
            Eigen::Matrix3f H3;
            H3 << hxx, hxy, hxs, hxy, hyy, hys, hxs, hys, hss;
            const float det3 = H3.determinant();
            if (std::fabs(det3) < 1e-12f) break;
            const Eigen::Vector3f step = -H3.inverse() * Eigen::Vector3f(gx, gy, gs);
            ox = step.x();
            oy = step.y();
            os = step.z();
            if (std::fabs(ox) < 0.6f && std::fabs(oy) < 0.6f && std::fabs(os) < 0.6f) {
              converged = true;
              break;
            }
            const int nx = rx_i + (ox > 0.6f ? 1 : (ox < -0.6f ? -1 : 0));
            const int ny = ry_i + (oy > 0.6f ? 1 : (oy < -0.6f ? -1 : 0));
            const int ns = rs_i + (os > 0.6f ? 1 : (os < -0.6f ? -1 : 0));
            if (nx < 4 || nx >= ow - 4 || ny < 4 || ny >= oh - 4 || ns < 1 || ns > S) break;
            if (nx == rx_i && ny == ry_i && ns == rs_i) {
              converged = true;
              break;
            }
            rx_i = nx;
            ry_i = ny;
            rs_i = ns;
          }
          if (!converged || std::fabs(ox) > 1.5f || std::fabs(oy) > 1.5f || std::fabs(os) > 1.5f) {
            continue;
          }

          const auto& d_m = dog[rs_i - 1];
          const auto& d_c = dog[rs_i];
          const auto& d_p = dog[rs_i + 1];
          const float vc = d_c[ry_i * ow + rx_i];
          const float gx = 0.5f * (d_c[ry_i * ow + rx_i + 1] - d_c[ry_i * ow + rx_i - 1]);
          const float gy = 0.5f * (d_c[(ry_i + 1) * ow + rx_i] - d_c[(ry_i - 1) * ow + rx_i]);
          const float gs = 0.5f * (d_p[ry_i * ow + rx_i] - d_m[ry_i * ow + rx_i]);
          const float resp = vc + 0.5f * (gx * ox + gy * oy + gs * os);
          if (std::fabs(resp) < vl_peak_thr) continue;

          const float dxx = d_c[ry_i * ow + rx_i + 1] + d_c[ry_i * ow + rx_i - 1] - 2.0f * vc;
          const float dyy = d_c[(ry_i + 1) * ow + rx_i] + d_c[(ry_i - 1) * ow + rx_i] - 2.0f * vc;
          const float dxy = 0.25f * (d_c[(ry_i + 1) * ow + rx_i + 1] - d_c[(ry_i + 1) * ow + rx_i - 1] -
                                     d_c[(ry_i - 1) * ow + rx_i + 1] + d_c[(ry_i - 1) * ow + rx_i - 1]);
          const float tr = dxx + dyy;
          const float det = dxx * dyy - dxy * dxy;
          if (det <= 1e-9f || (tr * tr) / det >= edge_ratio) continue;

          const float fx = rx_i + ox;
          const float fy = ry_i + oy;
          const float sigma_oct = sigma0 * std::pow(k_step, static_cast<float>(rs_i) + os);
          const auto& img = gauss[rs_i];

          // Dominant orientations (36-bin Gaussian-weighted histogram + 6x box smooth + parabolic peak)
          float hist[36] = {0.0f};
          const int ori_rad = std::clamp(static_cast<int>(std::round(3.0f * 1.5f * sigma_oct)), 2, 14);
          const float exp_den = 2.0f * (1.5f * sigma_oct) * (1.5f * sigma_oct);
          for (int yy = std::max(1, ry_i - ori_rad); yy <= std::min(oh - 2, ry_i + ori_rad); ++yy) {
            for (int xx = std::max(1, rx_i - ori_rad); xx <= std::min(ow - 2, rx_i + ori_rad); ++xx) {
              const float igx = img[yy * ow + xx + 1] - img[yy * ow + xx - 1];
              const float igy = img[(yy + 1) * ow + xx] - img[(yy - 1) * ow + xx];
              const float mag = std::hypot(igx, igy);
              const float wgt = std::exp(-((xx - fx) * (xx - fx) + (yy - fy) * (yy - fy)) / exp_den);
              float ang = std::atan2(igy, igx);
              if (ang < 0.0f) ang += 6.283185307f;
              const float fbin = ang * (36.0f / 6.283185307f) - 0.5f;
              const int b0 = (static_cast<int>(std::floor(fbin)) % 36 + 36) % 36;
              const float wb = fbin - std::floor(fbin);
              hist[b0] += mag * wgt * (1.0f - wb);
              hist[(b0 + 1) % 36] += mag * wgt * wb;
            }
          }
          for (int pass = 0; pass < 6; ++pass) {
            float tmp[36];
            for (int b = 0; b < 36; ++b) {
              tmp[b] = (hist[(b + 35) % 36] + hist[b] + hist[(b + 1) % 36]) * (1.0f / 3.0f);
            }
            std::memcpy(hist, tmp, sizeof(hist));
          }
          float max_h = 0.0f;
          for (int b = 0; b < 36; ++b) max_h = std::max(max_h, hist[b]);
          if (max_h <= 1e-12f) continue;

          for (int b = 0; b < 36; ++b) {
            const float hm = hist[(b + 35) % 36];
            const float hc = hist[b];
            const float hp = hist[(b + 1) % 36];
            if (hc < 0.8f * max_h || hc < hm || hc < hp) continue;
            const float denom = hm - 2.0f * hc + hp;
            const float di = (std::fabs(denom) > 1e-9f) ? std::clamp(0.5f * (hm - hp) / denom, -0.5f, 0.5f) : 0.0f;
            float angle = (static_cast<float>(b) + 0.5f + di) * (6.283185307f / 36.0f);
            if (angle < 0.0f) angle += 6.283185307f;
            if (angle >= 6.283185307f) angle -= 6.283185307f;

            const float cos_t = std::cos(angle);
            const float sin_t = std::sin(angle);

            // 4x4x8 = 128D SIFT descriptor in UBC bin ordering
            RawKp kp{};
            kp.x = (fx + 0.5f) * oct_scale;
            kp.y = (fy + 0.5f) * oct_scale;
            kp.scale = sigma_oct * oct_scale;
            kp.angle = angle;
            kp.response = std::fabs(resp);
            kp.octave = o;
            kp.level = rs_i;
            kp.desc.fill(0.0f);

            const float hist_width = 3.0f * sigma_oct;
            const int desc_rad = std::clamp(
                static_cast<int>(std::ceil(hist_width * 1.41421356f * 2.5f)), 3, 18);
            for (int dy_i = -desc_rad; dy_i <= desc_rad; ++dy_i) {
              const int yy = ry_i + dy_i;
              if (yy < 1 || yy >= oh - 1) continue;
              for (int dx_i = -desc_rad; dx_i <= desc_rad; ++dx_i) {
                const int xx = rx_i + dx_i;
                if (xx < 1 || xx >= ow - 1) continue;
                const float rx = ((xx - fx) * cos_t + (yy - fy) * sin_t) / hist_width;
                const float ry = (-(xx - fx) * sin_t + (yy - fy) * cos_t) / hist_width;
                const float bx = rx + 1.5f;
                const float by = ry + 1.5f;
                if (bx <= -1.0f || bx >= 4.0f || by <= -1.0f || by >= 4.0f) continue;

                const float dgx = img[yy * ow + xx + 1] - img[yy * ow + xx - 1];
                const float dgy = img[(yy + 1) * ow + xx] - img[(yy - 1) * ow + xx];
                float rot_ang = std::atan2(dgy, dgx) - angle;
                while (rot_ang < 0.0f) rot_ang += 6.283185307f;
                while (rot_ang >= 6.283185307f) rot_ang -= 6.283185307f;
                const float bo = rot_ang * (8.0f / 6.283185307f);
                const float mag = std::hypot(dgx, dgy) * std::exp(-0.125f * (rx * rx + ry * ry));

                const int x0 = static_cast<int>(std::floor(bx));
                const int y0 = static_cast<int>(std::floor(by));
                const int o0 = static_cast<int>(std::floor(bo));
                const float wx = bx - x0;
                const float wy = by - y0;
                const float wo = bo - o0;
                for (int iy = 0; iy < 2; ++iy) {
                  const int cy = y0 + iy;
                  if (cy < 0 || cy >= 4) continue;
                  const float v_y = mag * (iy == 0 ? (1.0f - wy) : wy);
                  for (int ix = 0; ix < 2; ++ix) {
                    const int cx = x0 + ix;
                    if (cx < 0 || cx >= 4) continue;
                    const float v_xy = v_y * (ix == 0 ? (1.0f - wx) : wx);
                    for (int io = 0; io < 2; ++io) {
                      const int co = (o0 + io) & 7;
                      const float v_xyo = v_xy * (io == 0 ? (1.0f - wo) : wo);
                      kp.desc[(cy * 4 + cx) * 8 + co] += v_xyo;
                    }
                  }
                }
              }
            }
            candidates.push_back(kp);
          }
        }
      }
    }

    // Downsample gauss[S] by 2x for next octave
    if (o + 1 < octaves) {
      const int nw = ow / 2;
      const int nh = oh / 2;
      if (nw < 24 || nh < 24) break;
      oct_base.resize(static_cast<size_t>(nw) * nh);
      for (int y = 0; y < nh; ++y) {
        for (int x = 0; x < nw; ++x) {
          oct_base[y * nw + x] = gauss[S][(2 * y) * ow + (2 * x)];
        }
      }
      ow = nw;
      oh = nh;
    }
  }

  std::sort(candidates.begin(), candidates.end(),
            [](const RawKp& a, const RawKp& b) { return a.response > b.response; });
  if (static_cast<int>(candidates.size()) > max_features) {
    // Spatial 4x4 grid bucketing so strong texture in one corner doesn't starve the rest of the frame.
    constexpr int kGridX = 4;
    constexpr int kGridY = 4;
    const int per_cell = std::max(1, max_features / (kGridX * kGridY));
    int cell_counts[kGridX * kGridY] = {0};
    std::vector<RawKp> kept;
    kept.reserve(static_cast<size_t>(max_features));
    std::vector<RawKp> overflow;
    for (const auto& c : candidates) {
      const int gx = std::clamp(static_cast<int>((c.x / std::max(1, width)) * kGridX), 0, kGridX - 1);
      const int gy = std::clamp(static_cast<int>((c.y / std::max(1, height)) * kGridY), 0, kGridY - 1);
      const int cell = gy * kGridX + gx;
      if (cell_counts[cell] < per_cell && static_cast<int>(kept.size()) < max_features) {
        kept.push_back(c);
        ++cell_counts[cell];
      } else {
        overflow.push_back(c);
      }
    }
    for (const auto& c : overflow) {
      if (static_cast<int>(kept.size()) >= max_features) break;
      kept.push_back(c);
    }
    candidates = std::move(kept);
  }

  keypoints->resize(candidates.size());
  descriptors->resize(static_cast<Eigen::Index>(candidates.size()), 128);
  for (size_t i = 0; i < candidates.size(); ++i) {
    const auto& c = candidates[i];
    (*keypoints)[i] = FeatureKeypoint(c.x, c.y, c.scale, c.angle);
    std::array<float, 128> d = c.desc;
    // Standard SIFT L2-clip-renormalize, followed by optional RootSIFT (L1-root)
    float l2 = 0.0f;
    for (float v : d) l2 += v * v;
    l2 = std::sqrt(std::max(l2, 1e-12f));
    float l2_2 = 0.0f;
    for (float& v : d) {
      v = std::min(0.2f, v / l2);
      l2_2 += v * v;
    }
    l2_2 = std::sqrt(std::max(l2_2, 1e-12f));
    for (float& v : d) v /= l2_2;
    if (l1_root_norm) {
      float l1 = 0.0f;
      for (float v : d) l1 += std::fabs(v);
      l1 = std::max(l1, 1e-12f);
      float root_l2 = 0.0f;
      for (float& v : d) {
        v = std::sqrt(std::fabs(v) / l1);
        root_l2 += v * v;
      }
      root_l2 = std::sqrt(std::max(root_l2, 1e-12f));
      for (float& v : d) v /= root_l2;
    }
    for (int k = 0; k < 128; ++k) {
      (*descriptors)(static_cast<Eigen::Index>(i), k) =
          static_cast<uint8_t>(std::clamp(std::round(512.0f * d[k]), 0.0f, 255.0f));
    }
  }
  return true;
}

bool OpenGLSiftEngine::Match(const FeatureDescriptorsData& desc1,
                             const FeatureDescriptorsData& desc2,
                             float max_ratio,
                             float max_distance,
                             bool cross_check,
                             FeatureMatches* matches) {
  if (!IsAvailable()) return false;
  matches->clear();
  const int n1 = static_cast<int>(desc1.rows());
  const int n2 = static_cast<int>(desc2.rows());
  if (n1 == 0 || n2 == 0) return true;

  auto& gl = GetGlApi();
  auto run_one_way = [&](const FeatureDescriptorsData& qa,
                         const FeatureDescriptorsData& db,
                         std::vector<int>* best_out) -> bool {
    const int na = static_cast<int>(qa.rows());
    const int nb = static_cast<int>(db.rows());
    const GLsizeiptr bytes_a = static_cast<GLsizeiptr>(na) * 128;
    const GLsizeiptr bytes_b = static_cast<GLsizeiptr>(nb) * 128;
    const GLsizeiptr bytes_idx = static_cast<GLsizeiptr>(na) * 2 * sizeof(int32_t);
    const GLsizeiptr bytes_dot = static_cast<GLsizeiptr>(na) * 2 * sizeof(float);

    gl.glBindBuffer(kGlShaderStorageBuffer, impl_->bufs[0]);
    gl.glBufferData(kGlShaderStorageBuffer, bytes_a, qa.data(), kGlDynamicDraw);
    gl.glBindBuffer(kGlShaderStorageBuffer, impl_->bufs[1]);
    gl.glBufferData(kGlShaderStorageBuffer, bytes_b, db.data(), kGlDynamicDraw);
    gl.glBindBuffer(kGlShaderStorageBuffer, impl_->bufs[2]);
    gl.glBufferData(kGlShaderStorageBuffer, bytes_idx, nullptr, kGlDynamicDraw);
    gl.glBindBuffer(kGlShaderStorageBuffer, impl_->bufs[3]);
    gl.glBufferData(kGlShaderStorageBuffer, bytes_dot, nullptr, kGlDynamicDraw);

    gl.glUseProgram(impl_->match_prog);
    gl.glUniform1i(gl.glGetUniformLocation(impl_->match_prog, "u_n1"), na);
    gl.glUniform1i(gl.glGetUniformLocation(impl_->match_prog, "u_n2"), nb);
    for (GLuint b = 0; b < 4; ++b) {
      gl.glBindBufferBase(kGlShaderStorageBuffer, b, impl_->bufs[b]);
    }
    gl.glDispatchCompute(static_cast<GLuint>((na + 63) / 64), 1, 1);
    gl.glMemoryBarrier(kGlShaderStorageBarrierBit | kGlBufferUpdateBarrierBit);

    gl.glBindBuffer(kGlShaderStorageBuffer, impl_->bufs[2]);
    const int32_t* mapped_idx = static_cast<const int32_t*>(
        gl.glMapBufferRange(kGlShaderStorageBuffer, 0, bytes_idx, kGlMapReadBit));
    if (!mapped_idx) return false;
    std::vector<int32_t> idx_copy(mapped_idx, mapped_idx + na * 2);
    gl.glUnmapBuffer(kGlShaderStorageBuffer);

    gl.glBindBuffer(kGlShaderStorageBuffer, impl_->bufs[3]);
    const float* mapped_dot = static_cast<const float*>(
        gl.glMapBufferRange(kGlShaderStorageBuffer, 0, bytes_dot, kGlMapReadBit));
    if (!mapped_dot) return false;
    std::vector<float> dot_copy(mapped_dot, mapped_dot + na * 2);
    gl.glUnmapBuffer(kGlShaderStorageBuffer);

    constexpr float kInvSqNorm = 1.0f / (512.0f * 512.0f);
    best_out->assign(na, -1);
    for (int i = 0; i < na; ++i) {
      const int b_idx = idx_copy[2 * i];
      if (b_idx < 0) continue;
      const float d1_norm = std::acos(std::clamp(dot_copy[2 * i] * kInvSqNorm, -1.0f, 1.0f));
      if (d1_norm > max_distance) continue;
      const float d2_norm = std::acos(std::clamp(dot_copy[2 * i + 1] * kInvSqNorm, -1.0f, 1.0f));
      if (d1_norm >= max_ratio * d2_norm) continue;
      (*best_out)[i] = b_idx;
    }
    return true;
  };

  std::vector<int> m12;
  if (!run_one_way(desc1, desc2, &m12)) return false;
  if (cross_check) {
    std::vector<int> m21;
    if (!run_one_way(desc2, desc1, &m21)) return false;
    for (int i = 0; i < n1; ++i) {
      const int j = m12[i];
      if (j >= 0 && j < n2 && m21[j] == i) {
        matches->emplace_back(i, j);
      }
    }
  } else {
    for (int i = 0; i < n1; ++i) {
      if (m12[i] >= 0) matches->emplace_back(i, m12[i]);
    }
  }
  return true;
}

}  // namespace colmap
