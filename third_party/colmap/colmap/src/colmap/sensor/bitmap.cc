// SPDX-License-Identifier: BSD-3-Clause
#include "colmap/sensor/bitmap.h"

#include "colmap/util/logging.h"
#include "colmap/util/string.h"

#include <algorithm>
#include <cmath>
#include <cstring>

namespace colmap {

Bitmap::Bitmap()
    : width_(0), height_(0), channels_(0), linear_colorspace_(false) {}

Bitmap::Bitmap(int width, int height, bool as_rgb, bool linear_colorspace)
    : width_(width),
      height_(height),
      channels_(as_rgb ? 3 : 1),
      linear_colorspace_(linear_colorspace),
      data_(static_cast<size_t>(std::max(0, width)) *
                static_cast<size_t>(std::max(0, height)) * (as_rgb ? 3 : 1),
            0) {}

Bitmap::Bitmap(const Bitmap& other)
    : width_(other.width_),
      height_(other.height_),
      channels_(other.channels_),
      linear_colorspace_(other.linear_colorspace_),
      data_(other.data_) {}

Bitmap::Bitmap(Bitmap&& other) noexcept = default;

Bitmap& Bitmap::operator=(const Bitmap& other) {
  if (this != &other) {
    width_ = other.width_;
    height_ = other.height_;
    channels_ = other.channels_;
    linear_colorspace_ = other.linear_colorspace_;
    data_ = other.data_;
  }
  return *this;
}

Bitmap& Bitmap::operator=(Bitmap&& other) noexcept = default;

void Bitmap::SetImageData(int width, int height, std::vector<uint8_t>&& data) {
  width_ = width;
  height_ = height;
  data_ = std::move(data);
}

void Bitmap::Fill(const BitmapColor<uint8_t>& color) {
  for (int y = 0; y < height_; ++y) {
    for (int x = 0; x < width_; ++x) {
      SetPixel(x, y, color);
    }
  }
}

Bitmap Bitmap::Clone() const { return *this; }

Bitmap Bitmap::CloneAsGrey() const {
  if (IsGrey()) return Clone();
  Bitmap out(width_, height_, false, linear_colorspace_);
  for (int y = 0; y < height_; ++y) {
    for (int x = 0; x < width_; ++x) {
      const auto px = GetPixel(x, y);
      if (px) {
        const uint8_t g = static_cast<uint8_t>(std::clamp(
            std::round(0.299 * px->r + 0.587 * px->g + 0.114 * px->b),
            0.0,
            255.0));
        out.SetPixel(x, y, BitmapColor<uint8_t>(g));
      }
    }
  }
  return out;
}

Bitmap Bitmap::CloneAsRGB() const {
  if (IsRGB()) return Clone();
  Bitmap out(width_, height_, true, linear_colorspace_);
  for (int y = 0; y < height_; ++y) {
    for (int x = 0; x < width_; ++x) {
      const auto px = GetPixel(x, y);
      if (px) {
        out.SetPixel(x, y, BitmapColor<uint8_t>(px->r, px->r, px->r));
      }
    }
  }
  return out;
}

void Bitmap::Rescale(int new_width, int new_height, RescaleFilter) {
  if (new_width <= 0 || new_height <= 0 || IsEmpty()) return;
  Bitmap dst(new_width, new_height, IsRGB(), linear_colorspace_);
  const double sx = static_cast<double>(width_) / new_width;
  const double sy = static_cast<double>(height_) / new_height;
  for (int y = 0; y < new_height; ++y) {
    const double src_y = std::clamp((y + 0.5) * sy - 0.5, 0.0, std::max(0.0, height_ - 1.001));
    for (int x = 0; x < new_width; ++x) {
      const double src_x = std::clamp((x + 0.5) * sx - 0.5, 0.0, std::max(0.0, width_ - 1.001));
      const auto c = InterpolateBilinear(src_x, src_y);
      if (c) {
        dst.SetPixel(x, y, c->Cast<uint8_t>());
      }
    }
  }
  *this = std::move(dst);
}

double Bitmap::Thumbnail(int max_image_size, RescaleFilter filter) {
  const int max_dim = std::max(width_, height_);
  if (max_dim <= max_image_size || max_image_size <= 0) return 1.0;
  const double scale = static_cast<double>(max_image_size) / max_dim;
  Rescale(std::max(1, static_cast<int>(std::round(width_ * scale))),
          std::max(1, static_cast<int>(std::round(height_ * scale))),
          filter);
  return scale;
}

void Bitmap::Rot90(int k) {
  k = ((k % 4) + 4) % 4;
  for (int i = 0; i < k; ++i) {
    Bitmap dst(height_, width_, IsRGB(), linear_colorspace_);
    for (int y = 0; y < height_; ++y) {
      for (int x = 0; x < width_; ++x) {
        if (auto c = GetPixel(x, y)) dst.SetPixel(y, width_ - 1 - x, *c);
      }
    }
    *this = std::move(dst);
  }
}

void Bitmap::Crop(int left, int top, int width, int height) {
  Bitmap dst(width, height, IsRGB(), linear_colorspace_);
  for (int y = 0; y < height; ++y) {
    for (int x = 0; x < width; ++x) {
      if (auto c = GetPixel(left + x, top + y)) dst.SetPixel(x, y, *c);
    }
  }
  *this = std::move(dst);
}

void Bitmap::SetJpegQuality(int) {}
void Bitmap::SetMetaData(const std::string_view&, const std::string_view&, const void*) {}
void Bitmap::SetMetaData(const std::string_view&, const std::string_view&) {}
bool Bitmap::GetMetaData(const std::string_view&, const std::string_view&, void*) const { return false; }
std::optional<std::string> Bitmap::GetMetaData(const std::string_view&) const { return std::nullopt; }
void Bitmap::CloneMetadata(Bitmap*) const {}

bool Bitmap::Read(const std::filesystem::path&, bool, bool) { return false; }
bool Bitmap::Write(const std::filesystem::path&, bool) const { return false; }

std::optional<int> Bitmap::ExifOrientation() const { return std::nullopt; }
std::optional<std::string> Bitmap::ExifCameraModel() const { return std::nullopt; }
std::optional<double> Bitmap::ExifFocalLength() const { return std::nullopt; }
std::optional<double> Bitmap::ExifLatitude() const { return std::nullopt; }
std::optional<double> Bitmap::ExifLongitude() const { return std::nullopt; }
std::optional<double> Bitmap::ExifAltitude() const { return std::nullopt; }

std::ostream& operator<<(std::ostream& stream, const Bitmap& bitmap) {
  stream << StringPrintf("Bitmap(width=%d, height=%d, channels=%d)",
                         bitmap.Width(), bitmap.Height(), bitmap.Channels());
  return stream;
}

float JetColormap::Red(const float gray) { return Base(gray - 0.25f); }
float JetColormap::Green(const float gray) { return Base(gray); }
float JetColormap::Blue(const float gray) { return Base(gray + 0.25f); }
BitmapColor<uint8_t> JetColormap::ToBitmapColor(const float gray) {
  return BitmapColor<uint8_t>(
      static_cast<uint8_t>(std::clamp(Red(gray) * 255.0f, 0.0f, 255.0f)),
      static_cast<uint8_t>(std::clamp(Green(gray) * 255.0f, 0.0f, 255.0f)),
      static_cast<uint8_t>(std::clamp(Blue(gray) * 255.0f, 0.0f, 255.0f)));
}
float JetColormap::Interpolate(float val, float y0, float x0, float y1, float x1) {
  return (val - x0) * (y1 - y0) / (x1 - x0) + y0;
}
float JetColormap::Base(const float val) {
  if (val <= -0.75f) return 0.0f;
  if (val <= -0.25f) return Interpolate(val, 0.0f, -0.75f, 1.0f, -0.25f);
  if (val <= 0.25f) return 1.0f;
  if (val <= 0.75f) return Interpolate(val, 1.0f, 0.25f, 0.0f, 0.75f);
  return 0.0f;
}

}  // namespace colmap
