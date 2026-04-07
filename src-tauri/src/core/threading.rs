pub(crate) fn thread_prefix(scope: &str) -> String {
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("unnamed");
    format!("[{}][thread={:?} name={}]", scope, thread.id(), thread_name)
}