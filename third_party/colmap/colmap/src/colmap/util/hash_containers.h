// SPDX-License-Identifier: BSD-3-Clause
#pragma once

#include <cstddef>
#include <cstdint>
#include <functional>
#include <type_traits>
#include <unordered_map>
#include <unordered_set>

namespace colmap {
namespace detail {

template <class Key, class Enable = void>
struct DefaultColmapHash : std::hash<Key> {};

template <class Key>
struct DefaultColmapHash<Key, std::enable_if_t<std::is_integral_v<Key> || std::is_enum_v<Key>>> {
  size_t operator()(const Key& k) const noexcept {
    uint64_t x = static_cast<uint64_t>(k);
    x = (x ^ (x >> 30)) * 0xbf58476d1ce4e5b9ULL;
    x = (x ^ (x >> 27)) * 0x94d049bb133111ebULL;
    x = x ^ (x >> 31);
    return static_cast<size_t>(x);
  }
};

}  // namespace detail

template <class Key,
          class Value,
          class Hash = detail::DefaultColmapHash<Key>,
          class Eq = std::equal_to<Key>>
using FlatHashMap = std::unordered_map<Key, Value, Hash, Eq>;

template <class Key,
          class Hash = detail::DefaultColmapHash<Key>,
          class Eq = std::equal_to<Key>>
using FlatHashSet = std::unordered_set<Key, Hash, Eq>;

template <class Key,
          class Value,
          class Hash = detail::DefaultColmapHash<Key>,
          class Eq = std::equal_to<Key>>
using NodeHashMap = std::unordered_map<Key, Value, Hash, Eq>;

template <class Key,
          class Hash = detail::DefaultColmapHash<Key>,
          class Eq = std::equal_to<Key>>
using NodeHashSet = std::unordered_set<Key, Hash, Eq>;

}  // namespace colmap
