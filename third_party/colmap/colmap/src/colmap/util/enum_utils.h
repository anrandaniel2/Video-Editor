// SPDX-License-Identifier: BSD-3-Clause
#pragma once

#include <ostream>
#include <stdexcept>
#include <string>
#include <string_view>
#include <vector>

namespace colmap::enum_detail {

inline std::vector<std::string_view> SplitEnumNames(std::string_view raw) {
  std::vector<std::string_view> out;
  size_t pos = 0;
  while (pos < raw.size()) {
    size_t comma = raw.find(',', pos);
    std::string_view tok = (comma == std::string_view::npos)
                               ? raw.substr(pos)
                               : raw.substr(pos, comma - pos);
    while (!tok.empty() && (tok.front() == ' ' || tok.front() == '\t' ||
                            tok.front() == '\n' || tok.front() == '\r')) {
      tok.remove_prefix(1);
    }
    while (!tok.empty() && (tok.back() == ' ' || tok.back() == '\t' ||
                            tok.back() == '\n' || tok.back() == '\r')) {
      tok.remove_suffix(1);
    }
    if (!tok.empty()) {
      out.push_back(tok);
    }
    if (comma == std::string_view::npos) {
      break;
    }
    pos = comma + 1;
  }
  return out;
}

}  // namespace colmap::enum_detail

#define DEFINE_ENUM_TO_FROM_STRING(name, start_idx, ...)                       \
  [[maybe_unused]] static const std::vector<std::string_view>&                 \
      name##RawNames() {                                                       \
    static const std::vector<std::string_view> names =                         \
        ::colmap::enum_detail::SplitEnumNames(#__VA_ARGS__);                   \
    return names;                                                              \
  }                                                                            \
  [[maybe_unused]] static std::string_view name##ToString(int value) {         \
    const auto& names = name##RawNames();                                      \
    const int idx = value - (start_idx);                                       \
    if (idx >= 0 && static_cast<size_t>(idx) < names.size()) {                 \
      return names[static_cast<size_t>(idx)];                                  \
    }                                                                          \
    throw std::runtime_error("Unknown value: " + std::to_string(value) +       \
                             " for enum: " #name);                             \
  }                                                                            \
  [[maybe_unused]] static std::string_view name##ToString(name value) {        \
    return name##ToString(static_cast<int>(value));                            \
  }                                                                            \
  [[maybe_unused]] static name name##FromString(std::string_view str) {        \
    const auto& names = name##RawNames();                                      \
    for (size_t i = 0; i < names.size(); ++i) {                                \
      if (names[i] == str) {                                                   \
        return static_cast<name>(static_cast<int>(i) + (start_idx));           \
      }                                                                        \
    }                                                                          \
    throw std::runtime_error("Unknown string value: " + std::string(str) +     \
                             " for enum: " #name);                             \
  }                                                                            \
  [[maybe_unused]] static std::vector<name> name##Values(                      \
      bool (*filter)(name) = nullptr) {                                        \
    const auto& names = name##RawNames();                                      \
    std::vector<name> out;                                                     \
    out.reserve(names.size());                                                 \
    for (size_t i = 0; i < names.size(); ++i) {                                \
      const name v = static_cast<name>(static_cast<int>(i) + (start_idx));     \
      if (filter == nullptr || filter(v)) {                                    \
        out.push_back(v);                                                      \
      }                                                                        \
    }                                                                          \
    return out;                                                                \
  }                                                                            \
  [[maybe_unused]] static std::vector<std::string_view> name##Strings(         \
      bool (*filter)(name) = nullptr) {                                        \
    const std::vector<name> values = name##Values(filter);                     \
    std::vector<std::string_view> strings;                                     \
    strings.reserve(values.size());                                            \
    for (const name value : values) {                                          \
      strings.push_back(name##ToString(value));                                \
    }                                                                          \
    return strings;                                                            \
  }

#define COLMAP_FIRST_ENUM_ARG(first, ...) first
#define COLMAP_REST_ENUM_ARGS(first, ...) __VA_ARGS__

#define MAKE_ENUM(name, start_idx, first_elem, ...)                            \
  enum name { first_elem = (start_idx), __VA_ARGS__ };                         \
  DEFINE_ENUM_TO_FROM_STRING(name, start_idx, first_elem, __VA_ARGS__)

#define MAKE_ENUM_CLASS(name, start_idx, first_elem, ...)                      \
  enum class name { first_elem = (start_idx), __VA_ARGS__ };                   \
  DEFINE_ENUM_TO_FROM_STRING(name, start_idx, first_elem, __VA_ARGS__)

#define MAKE_ENUM_CLASS_OVERLOAD_STREAM(name, start_idx, ...)                  \
  MAKE_ENUM_CLASS(name, start_idx, __VA_ARGS__);                               \
  inline std::ostream& operator<<(std::ostream& os, name value) {              \
    return os << name##ToString(value);                                        \
  }
