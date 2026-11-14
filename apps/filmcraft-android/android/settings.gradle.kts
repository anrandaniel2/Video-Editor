pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
        mavenCentral()
        gradlePluginPortal()
    }
}

@Suppress("UnstableApiUsage")
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "FilmCraft"
include(":app")

include(":procraft")
project(":procraft").projectDir = file("../../../third_party/procraft/android/app")

// The vendored PhotoCraft engine: Rust compiled for Android, packaged as a library. It has no UI of
// its own; FilmCraft's PhotoCraft screen drives it.
include(":photocraft")
project(":photocraft").projectDir = file("../../../third_party/photocraft/android/app")
