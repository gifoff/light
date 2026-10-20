//! Коллекция значений в духе `Illuminate\Support\Collection` из Laravel.
//!
//! Коллекция — это удобная обёртка над `Vec<T>` с цепочкой методов
//! (`map`, `filter`, `each`, `reduce`, …), каждый из которых возвращает новую
//! коллекцию. Дополнительно реализованы `Iterator`/`FromIterator`/
//! `IntoIterator`/`Deref`-интеграции, поэтому коллекцию можно передавать туда,
//! где ожидается срез или итератор.

use std::ops::{Deref, Index};

/// Упорядоченная коллекция значений.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection<T> {
    items: Vec<T>,
}

impl<T> Collection<T> {
    /// Создаёт пустую коллекцию.
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Оборачивает существующий вектор.
    pub fn from_vec(items: Vec<T>) -> Self {
        Self { items }
    }

    /// Добавляет значение в конец и возвращает коллекцию для цепочки вызовов.
    pub fn push(&mut self, value: T) -> &mut Self {
        self.items.push(value);
        self
    }

    /// Добавляет несколько значений из итератора.
    pub fn push_many(&mut self, values: impl IntoIterator<Item = T>) -> &mut Self {
        self.items.extend(values);
        self
    }

    /// Количество элементов.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Пуста ли коллекция.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Непустая ли коллекция.
    pub fn is_not_empty(&self) -> bool {
        !self.items.is_empty()
    }

    /// Количество элементов (синоним [`Collection::len`]).
    pub fn count(&self) -> usize {
        self.items.len()
    }

    /// Элемент по индексу.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.items.get(index)
    }

    /// Изменяемая ссылка на элемент по индексу.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.items.get_mut(index)
    }

    /// Первый элемент.
    pub fn first(&self) -> Option<&T> {
        self.items.first()
    }

    /// Последний элемент.
    pub fn last(&self) -> Option<&T> {
        self.items.last()
    }

    /// Коллекция как срез.
    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    /// Коллекция как изменяемый срез.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.items
    }

    /// Итератор по ссылкам на элементы.
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }

    /// Изменяемый итератор по элементам.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.items.iter_mut()
    }

    /// Извлекает внутренний `Vec`.
    pub fn into_vec(self) -> Vec<T> {
        self.items
    }

    /// Содержит ли коллекция заданное значение.
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        self.items.contains(value)
    }

    /// Первый элемент, удовлетворяющий предикату.
    pub fn find(&self, mut predicate: impl FnMut(&T) -> bool) -> Option<&T> {
        self.items.iter().find(|value| predicate(value))
    }

    /// Есть ли элемент, удовлетворяющий предикату.
    pub fn any(&self, predicate: impl FnMut(&T) -> bool) -> bool {
        self.items.iter().any(predicate)
    }

    /// Все ли элементы удовлетворяют предикату.
    pub fn all(&self, predicate: impl FnMut(&T) -> bool) -> bool {
        self.items.iter().all(predicate)
    }

    /// Количество элементов, удовлетворяющих предикату.
    pub fn count_where(&self, mut predicate: impl FnMut(&T) -> bool) -> usize {
        self.items.iter().filter(|value| predicate(value)).count()
    }

    /// Выполняет `each` для каждого элемента по ссылке.
    pub fn each(&self, mut action: impl FnMut(&T)) {
        for value in &self.items {
            action(value);
        }
    }

    /// Изменяет каждый элемент на месте.
    pub fn each_mut(&mut self, mut action: impl FnMut(&mut T)) {
        for value in &mut self.items {
            action(value);
        }
    }

    /// Преобразует каждый элемент в новое значение.
    pub fn map<U>(self, transform: impl FnMut(T) -> U) -> Collection<U> {
        Collection {
            items: self.items.into_iter().map(transform).collect(),
        }
    }

    /// Оставляет элементы, удовлетворяющие предикату.
    pub fn filter(self, mut predicate: impl FnMut(&T) -> bool) -> Self {
        Collection {
            items: self
                .items
                .into_iter()
                .filter(|value| predicate(value))
                .collect(),
        }
    }

    /// Убирает элементы, удовлетворяющие предикату.
    pub fn reject(self, mut predicate: impl FnMut(&T) -> bool) -> Self {
        Collection {
            items: self
                .items
                .into_iter()
                .filter(|value| !predicate(value))
                .collect(),
        }
    }

    /// Сводит коллекцию к одному значению, начиная с `initial`.
    pub fn reduce<A>(&self, initial: A, fold: impl FnMut(A, &T) -> A) -> A {
        self.items.iter().fold(initial, fold)
    }

    /// Соединяет строковые элементы через разделитель.
    pub fn join(&self, separator: &str) -> String
    where
        T: std::fmt::Display,
    {
        self.items
            .iter()
            .map(T::to_string)
            .collect::<Vec<_>>()
            .join(separator)
    }
}

impl<T> Default for Collection<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<Vec<T>> for Collection<T> {
    fn from(items: Vec<T>) -> Self {
        Self { items }
    }
}

impl<T> FromIterator<T> for Collection<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            items: iter.into_iter().collect(),
        }
    }
}

impl<T> Extend<T> for Collection<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.items.extend(iter);
    }
}

impl<T> IntoIterator for Collection<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a Collection<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut Collection<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.iter_mut()
    }
}

impl<T> Deref for Collection<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl<T> Index<usize> for Collection<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.items[index]
    }
}
