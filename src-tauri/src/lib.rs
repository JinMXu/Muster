#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod base64_util;
pub mod commands;
pub mod models;
pub mod services;
pub mod theme;
pub mod bootstrap;

pub use base64_util::base64_encode;