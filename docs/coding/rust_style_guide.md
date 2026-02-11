# RqCore Rust Style Guide

## Naming Conventions

- **Types**: PascalCase (e.g., `MessageBus`, `AccountSummary`)
- **Functions/Methods**: snake_case (e.g., `connect`, `server_time`)
- **Constants**: UPPER_SNAKE_CASE (e.g., `MAX_RECONNECT_ATTEMPTS`)
- **Module names**: snake_case (e.g., `market_data`, `order_management`)

## Comments

- **Keep comments concise and avoid redundancy**. Don't state the obvious.
  - ✅ Good: Complex logic that needs explanation
  - ❌ Bad: `Connection::new(); // Initialize connection`
  - ❌ Bad: `flag = true ;// Set flag to true`

## Folder structure

- For file names (and variable names), use **snake_case**: all lowercase, underscore (not the hyphen, because '-' can mean subtraction, so it is never good for variable names)

- For folder names: also lowercase, and use _ underscore in general. That is the Rust convention (although mixed). However, if the folder is a top-level folder that contains a crate (it has Cargo.toml in it), then the crate name is hyphen (-). E.g. Cargo name = "actix-router", then the folder name uses hyphen: e.g. "actix-router" (this is because the Cargo package manager treat names with hyphen, not underscore). This is the only exception in Rust convention.


## General code style

+ **Curly brackets {} are mandatory even in single statements** in Rust. It is Rust design, people voted for it. 
But code looks longer (3 lines instead of 2), but if there is a lot of them in a for() loop, then it is annoying to read.
It looks weird, but we can do this, if it is annoying:
if !self.is_trading_allowed { 
    continue; 
}
=> 
if !self.is_trading_allowed
    { continue; }


## How to do proper logging without unwrap() crashing at errors

Some notes before presenting the various code patterns.
  - 1. In production codes (e.g. actix-web), people minimize error logging, because file writing or printing to terminal slows the execution. We can log a bit more than production codes, but we shouldn't overdo it.
  - 2. 'Try' to avoid logging errors multiple times in nested function. If f1() calls f2() calls f3(), we don't want to log the error 3 times. Returning Error 3x times is fine. Just don't log it 3 times.

This pattern can be **used for `Result<T, T::Err>` (Ok, Err) and `Option<T>` (Some, None)** as well.
Aims:
  - 1. No panics in production code. No unwrap() or expect() that can panic the whole process.
  - 2. Try to avoid nested scopes as much as possible, to keep code readable.
  - 3. Keep code efficient, avoid unnecessary computations.

The following 5 programming patterns can be considered. **Use the first usually (if Err is needed)**, sometimes the second (if Err is not needed), the third C# style, just rarely.

```rust
'block_label: { // the code block scope can be labelled to use in break statements

let num_str = "a45.5";
// let num1 = num_str.parse::<f64>().unwrap(); // yes, this will panic and crash the whole process
// print!("test my price: {}", num1);

// 1. This is a good, efficient pattern to handle errors without panicking. 7 lines. Computationally optimal. And can continue the program flow afterwards. And there is no safe .unwrap() that stops the reader to think.
// *** Usage: Prefer this pattern in most cases.
let num: f64 = match num_str.parse::<f64>() {
    Ok(v) => v, //Ok() branch can be moved after Err(), but nobody does that usually.
    Err(e) => {
        log::error!("Error 1 {}", e); // error() go to console too; warn(), log() go only to logfile depend on logLevel
        break 'block_label; // return or "return e" or break (from inner loop) or break 'label (break from labelled block). Early return to avoid crashing.
    }
}; // num is available after this to continue program flow
print!("Parsed number: {}", num);

// 2. This is a pattern with 'let Ok()', not with 'match'. It is only good if we don't need the error object. Only 6 lines. Computationally optimal as well.
// *** Usage: If error object is not needed, this pattern is 1 line shorter.
let num =if let Ok(v) = num_str.parse::<f64>() {
    v
} else { // here we don't have the error object
    log::error!("Error 2 parsing number from string: {}", num_str);
    break 'block_label; // return or "return e" or break (from inner loop) or break 'label (break from labelled block). Early return to avoid crashing.
};
print!("Parsed number: {}", num);

// 3. C# style. This is a pattern with 'let Err()', not with 'match' and using safe .unwrap(). Only 6 lines. This is better than the parse_result.is_err() version, because that needs to get the error with another 1 line.
// Better to avoid it. Because later, it will be dificult to read the code and see a lot of .unwrap().
// ** Usage: not preferred, but we can accept this in the codebase ONLY IF there is a comment why unwrap() is safe here.
let parse_result = num_str.parse::<f64>();
if let Err(e) = parse_result {
    log::error!("Error 3 parsing number from string: {}. Error: {}", num_str, e);
    break 'block_label; // return or "return e" or break (from inner loop) or break 'label (break from labelled block). Early return to avoid crashing.
}
let num = parse_result.unwrap(); // safe to unwrap() or expect() now, because Err is handled 4 lines above.
print!("Parsed number: {}", num);

// 4. Same, but using .unwrap(), which is safe if we are sure it is not an error. Same 7 lines, but more compute on parse_result.
// * Usage: don't use this, as the previous 'let Err()' version is the same, but 1 line shorter.
let parse_result = num_str.parse::<f64>();
if parse_result.is_err() {
    let e = parse_result.err().unwrap(); // get the error object
    log::error!("Error 4 {}", e); // error() go to console too; warn(), log() go only to logfile depend on logLevel
    return; // return or "return e" or break (from inner loop) or break 'label (break from labelled block). Early return to avoid crashing.
}
let num = parse_result.unwrap(); // safe to unwrap() or expect() now, because Err is handled 4 lines above. You can use unwrap() and expect() if a comments assures that it is safe.
// let num = parse_result.into_ok(); // into_ok() or unwrap_infallible() cannot be used as (ParseFloatError) is not infallible - it can occur if parsing fails.
print!("Parsed number: {}", num);

// 5. In Rust, the ? operator is designed precisely for propagating errors by returning early from the function 
// with the Err variant, without logging or panicking—it simply hands the error back to the caller. 
// This only works if your function's return type error is compatible with the occured error.
let value = num_str.parse::<f64>()?;

// 6. If the called function generates an Option<T>, but our function has to return a Result<T, ErrString>, 
// then this pattern can be used to generate an error string.
// ok_or_else() transforms the Option<T> into a Result<T, E>, mapping Some(v) to Ok(v) and None to Err(err())
let (key, value) = line.split_once('=')
    .ok_or_else(|| format!("Invalid config format at line {}", line_no + 1))?;

// 7. If the called function generates an Result<T1,Err1>, but our function has to return a Result<T2, Err2>,
// so the error types has to be transformed, thes this pattern can be used to 'convert' Err2 to Err1 type:
let num: f64 = num_str.parse::<f64>().or(Err1("early error"))?;

// 8. If we have to handle errors, but don't want to return errors to the caller, then instead of expect() or unwrap()
// use unwrap_or_default(), unwrap_or(value) or unwrap_or_else(function_generating_value)
// Rust doc:
// fn expect(self, msg: &str) -> f64
// Returns the contained [Ok] value, consuming the self value.
// Because this function may panic, its use is generally discouraged. 
// Instead, prefer to use pattern matching and handle the [Err] case explicitly, 
// or call unwrap_or, unwrap_or_else, or unwrap_or_default.

// 9. Another way to swallow Errors is the .ok() macro
SERVER_APP_START_TIME.set(Utc::now());
// This raises a compiler warning: "unused `Result` that must be used. this `Result` may be an `Err` variant, which should be handled"
// .ok() converts errors to Options, and that can be swallowed without inspecting the Error state
SERVER_APP_START_TIME.set(Utc::now()).ok();
}
```

## String (&str) concat

Fastest/lowest-overhead for two &str is: one allocation, two appends.
```rust
let mut result = String::with_capacity(str1.len() + str2.len());
result.push_str(str1);
result.push_str(str2);
```
However, that is 3 lines, which is too much to read for simple concat.
So, instead, Array's concat is nearly as efficient (it also sums lengths internally), and allocates only once. It does basically the same thing as a one-liner:
```rust
[str1, str2].concat() // This is the suggested way!
```

Other slower ways (use them less often, and not in fast intended code. In init() functions, it is fine, when it only runs once):
let result: String = format!("{}{}", str1, str2); // this brings the formatter mechanism and more internal re-allocation as it appends more and more inputs.

let result: String = str1.to_owned() + str2; // The '+' operator works on String + &str, so own the first one.
Cons: Creates an intermediate String from the first &str.
