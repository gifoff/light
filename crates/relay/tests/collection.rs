//! Тесты коллекции в духе `Illuminate\Support\Collection`.

use relay::Collection;

#[test]
fn new_collection_is_empty() {
    let collection: Collection<i32> = Collection::new();
    assert!(collection.is_empty());
    assert!(!collection.is_not_empty());
    assert_eq!(collection.len(), 0);
    assert_eq!(collection.count(), 0);
    assert_eq!(collection.first(), None);
    assert_eq!(collection.last(), None);
}

#[test]
fn push_get_and_index() {
    let mut collection = Collection::new();
    collection.push(10).push(20).push(30);

    assert_eq!(collection.len(), 3);
    assert_eq!(collection.get(1), Some(&20));
    assert_eq!(collection[2], 30);
    assert_eq!(collection.first(), Some(&10));
    assert_eq!(collection.last(), Some(&30));

    *collection.get_mut(0).expect("элемент") = 11;
    assert_eq!(collection.first(), Some(&11));
}

#[test]
fn map_and_filter_chain() {
    let result: Vec<i32> = Collection::from(vec![1, 2, 3, 4, 5, 6])
        .filter(|value| value % 2 == 0)
        .map(|value| value * 10)
        .into_vec();
    assert_eq!(result, vec![20, 40, 60]);
}

#[test]
fn reject_drops_matching_values() {
    let result: Vec<i32> = Collection::from(vec![1, 2, 3, 4])
        .reject(|value| *value < 3)
        .into_vec();
    assert_eq!(result, vec![3, 4]);
}

#[test]
fn membership_and_search() {
    let collection = Collection::from(vec![5, 8, 13]);

    assert!(collection.contains(&8));
    assert!(!collection.contains(&7));
    assert_eq!(collection.find(|value| *value > 6), Some(&8));
    assert!(collection.any(|value| *value == 13));
    assert!(collection.all(|value| *value > 0));
    assert_eq!(collection.count_where(|value| *value > 6), 2);
}

#[test]
fn each_and_reduce() {
    let collection = Collection::from(vec![1, 2, 3]);

    let mut sum = 0;
    collection.each(|value| sum += value);
    assert_eq!(sum, 6);

    let total = collection.reduce(0, |acc, value| acc + value);
    assert_eq!(total, 6);
}

#[test]
fn join_connects_display_values() {
    let collection = Collection::from(vec!["а", "б", "в"]);
    assert_eq!(collection.join(", "), "а, б, в");
}

#[test]
fn from_iterator_and_extend() {
    let mut collection: Collection<i32> = (1..=3).collect();
    collection.extend([4, 5]);
    assert_eq!(collection.len(), 5);
    assert_eq!(collection.last(), Some(&5));
}

#[test]
fn deref_gives_slice_access() {
    let collection = Collection::from(vec![3, 1, 2]);
    assert_eq!(collection.as_slice(), &[3, 1, 2]);
    assert_eq!(collection.iter().max(), Some(&3));
    assert_eq!(collection.iter().copied().sum::<i32>(), 6);
}
