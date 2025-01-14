use std::{path::PathBuf, time::Duration};

use rstest::rstest;
use tokio::net::TcpListener;

mod common;

pub use common::*;

/// For running locally, so that new developers don't have the extra step of building the go app
/// before running the tests.
#[cfg(target_os = "macos")]
#[ctor::ctor]
fn build_go_app() {
    use std::{env, path::Path, process};
    let original_dir = env::current_dir().unwrap();
    let go_app_path = Path::new("tests/apps/app_go");
    env::set_current_dir(go_app_path).unwrap();
    let output = process::Command::new("go")
        .args(vec!["build", "-o", "19"])
        .output()
        .expect("Failed to build Go test app.");
    assert!(output.status.success(), "Building Go test app failed.");
    env::set_current_dir(original_dir).unwrap();
}

/// Start a web server injected with the layer, simulate the agent, verify expected messages from
/// the layer, send tcp messages and verify in the server output that the application received them.
/// Tests the layer's communication with the agent, the bind hook, and the forwarding of mirrored
/// traffic to the application.
#[rstest]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[timeout(Duration::from_secs(60))]
async fn test_mirroring_with_http(
    #[values(
        Application::PythonFlaskHTTP,
        Application::PythonFastApiHTTP,
        Application::NodeHTTP
    )]
    application: Application,
    dylib_path: &PathBuf,
) {
    let executable = application.get_executable().await; // Own it.
    println!("Using executable: {}", &executable);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    println!("Listening for messages from the layer on {addr}");
    let env = get_env(dylib_path.to_str().unwrap(), &addr);
    let mut test_process =
        TestProcess::start_process(executable, application.get_args(), env).await;

    // Accept the connection from the layer and verify initial messages.
    let mut layer_connection =
        LayerConnection::get_initialized_connection(&listener, application.get_app_port()).await;
    println!("Application subscribed to port, sending tcp messages.");

    layer_connection
        .send_connection_then_data("GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await;
    layer_connection
        .send_connection_then_data("POST / HTTP/1.1\r\nHost: localhost\r\n\r\npost-data")
        .await;
    layer_connection
        .send_connection_then_data("PUT / HTTP/1.1\r\nHost: localhost\r\n\r\nput-data")
        .await;
    layer_connection
        .send_connection_then_data("DELETE / HTTP/1.1\r\nHost: localhost\r\n\r\ndelete-data")
        .await;
    test_process.wait().await;
    test_process.assert_stdout_contains("GET: Request completed");
    test_process.assert_stdout_contains("POST: Request completed");
    test_process.assert_stdout_contains("PUT: Request completed");
    test_process.assert_stdout_contains("DELETE: Request completed");
    test_process.assert_no_error_in_stdout();
    test_process.assert_no_error_in_stderr();
}

/// Run the http mirroring test only on MacOS, because of a known crash on Linux.
#[cfg(target_os = "macos")]
#[rstest]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[timeout(Duration::from_secs(60))]
async fn test_mirroring_with_http_go(dylib_path: &PathBuf) {
    test_mirroring_with_http(Application::Go19HTTP, dylib_path).await;
}
