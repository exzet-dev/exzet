
use std::sync::{Arc, Mutex};
use hyperlight_common::flatbuffer_wrappers::function_types::{ParameterValue, ReturnType};
use hyperlight_host::{
    func::HostFunction1, sandbox_state::sandbox::EvolvableSandbox, sandbox_state::transition::Noop,
    MultiUseSandbox, UninitializedSandbox,
};

fn main() -> hyperlight_host::Result<()> {

    let guest_binary_path = "src/guests/bin/release/exzet-arch-guest";

    let mut uninitialized_sandbox = UninitializedSandbox::new(
        hyperlight_host::GuestBinary::FilePath(guest_binary_path.into()),
        None, // default configuration
        None, // default run options
        None, // default host print function
    )?;

    // Define a host function (example: sleep for 5 seconds)
    fn generic_host_log(message: String) -> hyperlight_host::Result<()> {
        println!("{}", message.to_string());
        Ok(())
    }

    let host_function = Arc::new(Mutex::new(generic_host_log));

    // Register the host function to make it callable by the guest
    host_function.register(&mut uninitialized_sandbox, "expLogOnHost")?;

    // Initialize the sandbox
    let mut sandbox: MultiUseSandbox = uninitialized_sandbox.evolve(Noop::default())?;

    // Define a message to send to the guest
    let message = "\nHello from the Brain! Waddup, im in a microvm.\n\n".to_string();

    // Call the "PrintOutput" function in the guest
    let result = sandbox.call_guest_function_by_name(
        "PrintOutput",                               // Guest function name
        ReturnType::Int,                         // Expected return type
        Some(vec![ParameterValue::String(message)]),      // Arguments to pass
    );

    match result {
        Ok(value) => println!("Guest function returned successfully: {:?}", value),
        Err(err) => println!("Error calling guest function: {:?}", err),
    }

    println!("Task complete. Cleaning up sandbox.");
    Ok(())
}
