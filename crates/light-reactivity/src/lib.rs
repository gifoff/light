//! # Light Reactivity — реактивное ядро Light
//!
//! Перебазирован из `light-fw-core::reactive` и `light-fw-core::collection`
//! в рамках декомпозиции 0.20.0 (этап 2, Strangler Fig).
//!
//! Модули:
//! * [`reactive`] — свойства, прокси, обозреватели, подписки (`watch`).
//! * [`collection`] — коллекция в духе `Illuminate\Support\Collection`.
//!
//! `light-fw-core` реэкспортирует оба модуля через
//! [`light_fw_core::reactive`] / [`light_fw_core::collection`], так что
//! существующий код продолжает работать без изменений.

#![deny(missing_docs)]

pub mod collection;
pub mod reactive;

pub use collection::Collection;
pub use reactive::{
    Property, Proxy, ProxyState, Watcher, set_repaint_requester, watch, with_observer,
};
