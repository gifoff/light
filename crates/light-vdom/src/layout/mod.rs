//! Раскладка виртуального дерева в прямоугольники, поиск узлов по точке
//! и вычисление областей перерисовки.

/// Раскладка дерева: константы, примитивы (`Point`, `Rect`, `ContentBox`),
/// `TextMetrics`, `PlacedNode` и структура `Layout`.
pub mod base;
/// Растягивание строк, выравнивание контейнеров, сортировка по z.
pub mod flow;
/// Внутренняя рекурсия hit-test.
pub mod hit_test;
/// Помещение узлов в поток (`place`).
pub mod place;
/// Построение раскладки (`build`) и методы запроса/поиска.
pub mod query;
/// Области перерисовки, слияние прямоугольников, утилиты.
pub mod regions;
#[cfg(test)]
mod tests;

// Re-export public API for backwards compatibility
pub use base::*;
pub use regions::{coalesce_regions, damage_regions, snapshot_rects};
