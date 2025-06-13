/// Async utilities.
use std::future::Future;

use tokio::task::JoinSet;

use crate::error::BarErr;

/// try_map spawns a future for each item in the iterator and waits for all of them to complete.
/// If any of the futures return an error, try_map will return that error.
/// It will run no more than 50 futures concurrently.
/// There is no guarantee that the order of the output will match the order of the input.
pub async fn try_map<T, I, F, O, Fut>(input: I, f: F) -> Result<Vec<O>, BarErr>
where
    I: IntoIterator<Item = T>,
    F: Fn(T) -> Fut + Send + 'static,
    Fut: Future<Output = Result<O, BarErr>> + Send + 'static,
    T: Send + Send + 'static,
    O: Send + 'static,
{
    let mut iterator = input.into_iter();
    let (lower_bound, _) = iterator.size_hint();
    let mut output = Vec::with_capacity(lower_bound);
    let mut set = JoinSet::new();

    for item in iterator.by_ref().take(50) {
        set.spawn(f(item));
    }

    while let Some(res) = set.join_next().await {
        match res.unwrap() {
            Ok(val) => {
                output.push(val);
                if let Some(item) = iterator.next() {
                    set.spawn(f(item));
                }
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    Ok(output)
}

/// try_for_each spawns a future for each item in the iterator and waits for all of them to complete.
/// If any of the futures return an error, try_for_each will return that error.
/// It will run no more than 50 futures concurrently.
pub async fn try_for_each<T, I, F, Fut>(input: I, f: F) -> Result<(), BarErr>
where
    I: IntoIterator<Item = T>,
    F: Fn(T) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), BarErr>> + Send + 'static,
    T: Send + Send + 'static,
{
    let mut iterator = input.into_iter();
    let mut set = JoinSet::new();

    for item in iterator.by_ref().take(50) {
        set.spawn(f(item));
    }
    while let Some(res) = set.join_next().await {
        let _: () = res??;
        if let Some(item) = iterator.next() {
            set.spawn(f(item));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[tokio::test]
    async fn test_try_map() {
        let input: Vec<usize> = (1..100).collect();
        let mut result = try_map(input.clone(), |x| async move { Ok(x * 2) })
            .await
            .unwrap();
        result.sort();
        assert_eq!(result, input.iter().map(|x| x * 2).collect::<Vec<usize>>());
    }

    #[tokio::test]
    async fn test_try_for_each() {
        let input: Vec<usize> = (1..100).collect();
        let sum: Arc<Mutex<usize>> = Arc::from(Mutex::from(0));
        let sum_clone = sum.clone();
        try_for_each(input, move |_| {
            let sum = sum.clone();
            async move {
                let mut sum = sum.lock().unwrap();
                *sum += 1;
                Ok(())
            }
        })
        .await
        .expect("should not fail");
        assert_eq!(*sum_clone.lock().unwrap(), 99);
    }
}
