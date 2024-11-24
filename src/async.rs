/// Async utilities.
use std::future::Future;

use tokio::task::JoinSet;

use crate::error::Errors;

/// try_map spawns a future for each item in the iterator and waits for all of them to complete.
/// If any of the futures return an error, try_map will return that error.
pub async fn try_map<T, I, F, O, Fut>(input: I, f: F) -> Result<Vec<O>, Errors>
where
    I: IntoIterator<Item = T>,
    F: Fn(T) -> Fut + Send + 'static,
    Fut: Future<Output = Result<O, Errors>> + Send + 'static,
    T: Send + Send + 'static,
    O: Send + 'static,
{
    let iterator = input.into_iter();
    let (lower_bound, _) = iterator.size_hint();
    let mut set = JoinSet::new();
    let mut output = Vec::with_capacity(lower_bound);

    for item in iterator {
        set.spawn(f(item));
    }

    while let Some(res) = set.join_next().await {
        match res.unwrap() {
            Ok(val) => {
                output.push(val);
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
pub async fn try_for_each<T, I, F, Fut>(input: I, f: F) -> Result<(), Errors>
where
    I: IntoIterator<Item = T>,
    F: Fn(T) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), Errors>> + Send + 'static,
    T: Send + Send + 'static,
{
    let mut set = JoinSet::new();

    for item in input {
        set.spawn(f(item));
    }

    while let Some(res) = set.join_next().await {
        let _ = res?;
    }

    Ok(())
}
