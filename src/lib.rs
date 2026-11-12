#![cfg_attr(test, allow(dead_code))]

#[cfg(any(target_arch = "wasm32", test))]
mod camera;
#[cfg(any(target_arch = "wasm32", test))]
mod composer;
#[cfg(any(target_arch = "wasm32", test))]
mod events;
#[cfg(any(target_arch = "wasm32", test))]
mod render;
#[cfg(any(target_arch = "wasm32", test))]
mod scene;
#[cfg(any(target_arch = "wasm32", test))]
mod sculptor;
#[cfg(any(target_arch = "wasm32", test))]
mod text;
#[cfg(target_arch = "wasm32")]
mod web;
