/// Async utilities.
use std::future::Future;

use tokio::task::JoinSet;

use crate::error::Errors;

/// try_map spawns a future for each item in the input vector and waits for all of them to complete.
/// If any of the futures return an error, try_map will return that error.
pub async fn try_map<T, F, O, Fut>(input: Vec<T>, f: F) -> Result<Vec<O>, Errors>
where
    F: Fn(T) -> Fut + Send + 'static,
    Fut: Future<Output = Result<O, Errors>> + Send + 'static,
    T: Send + Send + 'static,
    O: Send + 'static,
{
    let mut set = JoinSet::new();
    let mut output = Vec::with_capacity(input.len());

    for item in input {
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

/// try_for_each spawns a future for each item in the input vector and waits for all of them to complete.
/// If any of the futures return an error, try_for_each will return that error.
pub async fn try_for_each<T, F, Fut>(input: Vec<T>, f: F) -> Result<(), Errors>
where
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
