# Help wanted

As the zellij code-base changed, a lot of places where a call to `unwrap()`
previously made sense can now potentially cause errors which we'd like to
handle. While we don't consider `unwrap` to be a bad thing in general, it hides
the underlying error and leaves the user only with a stack trace to go on.
Worse than this, it will crash the application without giving us a chance to
potentially recover. This is particularly bad when the user is using
long-running sessions to perform tasks.

Hence, we would like to eliminate `unwrap()` statements from the code where
possible, and apply better error handling instead. This way, functions higher
up in the call stack can react to errors from underlying functions and either
try to recover, or give some meaningful error messages if recovery isn't
possible.

Since the zellij codebase is pretty big and growing rapidly, this endeavor
will continue to be pursued over time, as zellij develops. The idea is that
modules or single files are converted bit by bit, preferably in small PRs that
each target a specific module or file. **If you are looking to contribute to
zellij, this may be an ideal start for you!** This way you get to know the
codebase and get an idea which modules are used at which other places in the
code.

If you have an interest in this, don't hesitate to get in touch with us and
refer to the [tracking issue][tracking_issue] to see what has already been
done.

# Error handling facilities

You get access to all the relevant functions and traits mentioned in the
remainder of this document by including/adding this in the code you're working
on:

```rust
use zellij_utils::errors::prelude::*;
```

## Displaying panic messages

Panics are generally handled via the `Panic` error type and the
[`handle_panic`][handle_panic] panic handler function. The fancy formatting
is performed by the [`miette`][miette] crate.

## Propagating errors

We use the [`anyhow`][anyhow] crate to propagate errors up the call stack. At
the moment, zellij doesn't have custom error types, so we wrap whatever errors
the underlying libraries give us, if any. [`anyhow`][anyhow] serves the purpose
of providing [`context`][context] about where (i.e. under which circumstances)
an error happened.

A critical requirement for propagating errors is that all functions involved
must return the [`Result`][result] type. This allows convenient error handling
with the `?` operator.

At some point you will likely stop propagating errors and decide what to do
with the error. Generally you can:

1. Try to recover from the error, or
2. Report the error to the user and either
    1. Terminate program execution (See [`fatal`][fatal]), or
    2. Continue program execution (See [`non_fatal`][non_fatal])

## Handling errors

Ideally, when the program encounters an error it will try to recover as good as
it can. This can mean falling back to some sane default if a specific value
(e.g. an environment variable) cannot be found. Note that this isn't always
applicable. If in doubt, don't hesitate to ask.

Recovery usually isn't an option if an operation has changed the internal state
(i.e. the value or content of specific variables) of objects in the code. In
this case, if an error is encountered, it is best to declare the program state
corrupted and terminate the whole application. This can be done by `unwrap`ing
on the [`Result`][result] type. Always try to propagate the error as good as
you can and attach meaningful context before `unwrap`ing. This gives the user
an idea what went wrong and can also help developers in quickly identifying
which parts of the code to debug if necessary.

When you encounter such a fatal error and cannot propagate it further up (e.g.
because the current function cannot be changed to return a [`Result`][result],
or because it is the "root" function of a program thread), use the
[`fatal`][fatal] function to panic the application. It will attach some small
context to the error and finally `unwrap` it. Using this function over the
regular `unwrap` has the added benefit that other developers seeing this in the
code know that someone has previously spent some thought about error handling
at this location.

If you encounter a non-fatal error, use the [`non_fatal`][non_fatal] function
to handle it. Instead of `panic`ing the application, the error is written to
the application log and execution continues. Please use this sparingly, as an
error usually calls for actions to be taken rather than ignoring it.



# Examples of applied error handling

You can have a look at the commit that introduced error handling to the
`zellij_server::screen` module [right here][1] (look at the changes in
`zellij-server/src/screen.rs`). We'll use this to demonstrate a few things in
the following text. You can find countless other examples in the [tracking
issue for error handling][3]


## Converting a function to return a `Result` type

> **TL;DR**  
> - Add `use zellij_utils::errors::prelude::*;` to the file
> - Make the function return `Result<T>`, with an appropriate `T` (Often `()`)
> - Append `.context()` to any `Result` you get with a sensible error description (see below)
> - Generate ad-hoc errors with `anyhow!(<SOME MESSAGE>)`

Here's an example of the `Screen::render` function as it looked before:

```rust
    pub fn render(&mut self) {
        // ...
        let serialized_output = output.serialize();
        self.bus
            .senders
            .send_to_server(ServerInstruction::Render(Some(serialized_output)))
            .unwrap();
    }
```

It performs a few actions (not shown here for brevity) and then sends an IPC
message to the server. As you can see it calls `unwrap()` on the result from
sending a message to the server. This means: If sending the message to the
server fails, execution is terminated and the program crashes. Let's assume
that crashing the application in this case is a reasonable course of action.

In total (as of writing this), the `render()` function is called 80 times from
various places in the code of the `Screen` struct. Hence, if sending the
message fails, we only see that the application crashed trying to send an IPC
message to the server. We won't know which of the 80 different code paths lead
to the execution of this function.

So what can we do? Instead of `unwrap`ing on the `Result` type here, we can
pass it up to the calling functions. Each of the callers can then decide for
themselves what to do: Continue regardless, propagate the error further up or
terminate execution.

Here's what the function looked like after the change linked above:

```rust
    pub fn render(&mut self) -> Result<()> {
        let err_context = || "failed to render screen".to_string();
        // ...

        let serialized_output = output.serialize();
        self.bus
            .senders
            .send_to_server(ServerInstruction::Render(Some(serialized_output)))
            .with_context(err_context)
    }
```

We leverage the [`Context`][context] trait from [`anyhow`][anyhow] to attach a
context message to the error and make the function return a `Result` type
instead. As you can see, the `Result` here contains a `()`, which is the empty
type. It's primary purpose here is allowing us to propagate errors to callers
of this function.

Hence, for example the `resize_to_screen` function changed from this:

```rust
    pub fn resize_to_screen(&mut self, new_screen_size: Size) {
        // ...
        self.render();
    }
```

to this:

```rust
    pub fn resize_to_screen(&mut self, new_screen_size: Size) -> Result<()> {
        // ...
        self.render()
            .with_context(|| format!("failed to resize to screen size: {new_screen_size:#?}"))
    }
```

Note how it returns a `Result` type now, too. This way we can pass the error up
to callers of `resize_to_screen` and keep going like this until we decide it's
time to do something about the error.

In general, any function calling `unwrap` or `expect` is a good candidate to be
rewritten to return a `Result` type instead.


## Attaching context

[Anyhow][anyhow]s [`Context`][context] trait gives us two methods to attach
context to an error: `context` and `with_context`. You should use `context`
if the message contains only a static text and `with_context` if you need
additional formatting:

```rust
    fn move_clients_between_tabs(
        &mut self,
        source_tab_index: usize,
        destination_tab_index: usize,
        clients_to_move: Option<Vec<ClientId>>,
    ) -> Result<()> {
        // ...
        if let Some(client_mode_info_in_source_tab) = drained_clients {
            let destination_tab = self.get_indexed_tab_mut(destination_tab_index)
                .context("failed to get destination tab by index")
                .with_context(|| format!("failed to move clients from tab {source_tab_index} to tab {destination_tab_index}"))?;
            // ...
        }
        Ok(())
    }
```

Feel free to move context string/closure to a variable to avoid copy-pasting:

```rust
    pub fn render(&mut self) -> Result<()> {
        let err_context = "failed to render screen";
        // ...

        for tab_index in tabs_to_close {
            // ...
            self.close_tab_at_index(tab_index)
                .context(err_context)?;
        }
        // ...
        self.bus
            .senders
            .send_to_server(ServerInstruction::Render(Some(serialized_output)))
            .context(err_context)
    }
    // ...
    pub fn close_tab(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || format!("failed to close tab for client {client_id:?}");

        let active_tab_index = *self
            .active_tab_indices
            .get(&client_id)
            .with_context(err_context)?;
        self.close_tab_at_index(active_tab_index)
            .with_context(err_context)
    }
```

When there is only a single `Result` to be returned from your function, use
`context` as shown above for `resize_to_screen`.


## Choosing helpful context messages

> **TL;DR**
> - Don't repeat what the error message in the `Result` says
> - Describe what you were doing, ideally include the current functions name

When attaching context to an error, usually you want to express what you were
doing when the error occurred, i.e. in what context the error occurred. In the
`render` method, we could have done something like this instead:

```rust
    pub fn render(&mut self) -> Result<()> {
        // ...

        for tab_index in tabs_to_close {
            // ...
            self.close_tab_at_index(tab_index)
                .context("Failed to close tab at index: {tab_index}")?;
        }
        // ...
        self.bus
            .senders
            .send_to_server(ServerInstruction::Render(Some(serialized_output)))
            .context("Failed to send message to server")
    }
```

Why do we add the message "failed to render screen" instead? Because that is
what we were trying to do when we received the error from the underlying
functions (`close_tab_at_index` and `send_to_server` in this case). Functions
from libraries usually already return an error that describes what went wrong
(Example: When we try to open a file that doesn't exist, the std library will
give us a [`NotFound`][2] error), so we don't have to repeat that.

In case of doubt, look at the name of the function you're currently working in
and write a context message somehow mentioning this.


## Terminating execution

> **TL;DR**
> - Terminate execution on errors by adding `.fatal()` to it
> - First try to pass the error as far up as you can or deem reasonable

We want to propagate errors as far up as we can. This way, every function along
the way can at least attach a context message giving us an idea what chain of
events lead to the error. Where do we terminate execution in `Screen`? If you
study the code in `screen.rs`, you'll notice all the components of zellij
interact with the `Screen` instance by means of IPC messages. These messages
are handled in the `screen_thread_main` function. Here's an excerpt:

```rust
    ScreenInstruction::Render => {
        screen.render()?;
    },
    ScreenInstruction::NewPane(pid, client_or_tab_index) => {
        // ...
        screen.update_tabs()?;

        screen.render()?;
    },
    ScreenInstruction::OpenInPlaceEditor(pid, client_id) => {
        // ...
        screen.update_tabs()?;

        screen.render()?;
    },
```

The code goes on like this for quite a while, so there are many places where an
error may occur. In this case, since all the functions are called from this
central location, we forego attaching a context message to every error.
Instead, we propagate the errors to the caller of this function, which happens
to be the function `init_session` in `zellij-server/src/lib.rs`. We see that
`screen_thread_main` is spawned to run in a separate thread. Hence, we cannot
propagate the error further up and terminate execution at this point:

```rust
    // ...
    screen_thread_main(
        screen_bus,
        max_panes,
        client_attributes_clone,
        config_options,
    )
    .fatal();
```

Remember the call to [`fatal`][fatal] will log the error and afterwards panic
the application (i.e. crash zellij). Since we made sure to attach context
messages to the errors on their way up, we will see these messages in the
resulting output!


## Error handling for `Option` types

> **TL;DR**
> - Attach a `.context` with a message saying why a `None` here is an error
> - Attach a regular context message like you would for a `Result` type, too!

Beyond what's described in "Choosing helpful context messages" above, `Option`
types benefit from extra handling. That is because a `Option` containing a
`None` where a value is expected doesn't carry an error message: It doesn't
tell us why the `None` is bad (i.e. equivalent to an Error) in this case.

In situations where a call to `unwrap()` or similar on a `Option` type is to be
converted for error handling, it is a good idea to attach an additional short
context. An example from the zellij codebase is shown below:

```rust
    let destination_tab = self.get_indexed_tab_mut(destination_tab_index)
        .context("failed to get destination tab by index")
        .with_context(|| format!("failed to move clients from tab {source_tab_index} to tab {destination_tab_index}"))?;
```

Here the call to `self.get_indexed_tab_mut(destination_tab_index)` will return
a `Option`. The surrounding code, however, doesn't know what to do with a
`None` value, so it is considered an error.

Here you see that we attach two contexts:

```rust
        .context("failed to get destination tab by index")
```

Because the `None` type itself doesn't tell us what the "error" means, we
attach a description manually. The second context:

```rust
        .with_context(|| format!("failed to move clients from tab {source_tab_index} to tab {destination_tab_index}"))?;
```

then describes what the surrounding function was trying to achieve (See
descriptions above).


## Logging errors

> **TL;DR**  
> - When there's a `Result` type around, use `.non_fatal()` on that instead of `log::error!`
> - When there's a `Err` type around, use `Err::<(), _>(err).non_fatal()`
> - Also attach context before logging!
> - For further examples, refer to [PR #1881][pr1881]

You may encounter situations where you have an error and decide it's safe to
ignore. Depending on the circumstances, this is a perfectly fine thing to do.
However, oftentimes it proves to be useful to at least log the error, so in
case things do go wrong we at least see the logged error message. Also, the
logged message may hint towards an underlying problem which may require further
action.

An obvious thing to do is something like the following:

```rust
log::error!("failed to find tab with index {tab_index}");
```

While an ad-hoc log message is better than silently ignoring the error, we can
usually do better than that. That is because in large parts of the codebase we
have a `Result` available in one form or another.

If the `Result` has been treated as suggested above and context messages have
been attached to it, it already contains a lot of valuable information. This is
lost when instead we log a custom error. Hence, the better solution is to log
the `Result` type including all the context information.

This is easily achieved like so:

```rust
fs::create_dir_all(&plugin_global_data_dir)
    .context("failed to create plugin asset directory")
    .non_fatal();
```

Here, we try to create a directory, and if we fail to do this we simply log the
error (with `non_fatal()`) and continue. It's important to note that the
`non_fatal()` function always returns `()`: It cannot be used on any `Result`
type whose `Ok` value is different from `()`. This is on purpose: If your
`Result` carries a type, it is probably required for further
calculations/actions. Hence, you mustn't ignore it.

Also note that, even though we log the error and continue, we still attach
context information to it. Just like with fatal errors, the context information
allows us to tell what we tried to do before we got the error and logged it.

This is a simple example, and oftentimes the `Result` you're trying to handle
doesn't carry a `()`. Your code may look more like this:

```rust
if let Ok(active_tab) = self.get_active_tab(client_id) {
    let active_tab_pos = active_tab.position;
    let new_tab_pos = (active_tab_pos + 1) % self.tabs.len();
    return self.switch_active_tab(new_tab_pos, client_id);
} else {
    log::error!("Active tab not found for client_id: {:?}", client_id);
}
```

When we get an `Ok`, we do something with it. When we get an `Err`, we log a
message. Here you'll notice that the usage of an `if let` statement hides the
error from us: In the `else` branch, we have no means to access the error we
got from `get_active_tab()` above. In such a situation, it is helpful to
rewrite the `if let` to a `match` statement instead:

```rust
match self.get_active_tab(client_id) {
    Ok(active_tab) => {
        let active_tab_pos = active_tab.position;
        let new_tab_pos = (active_tab_pos + 1) % self.tabs.len();
        return self.switch_active_tab(new_tab_pos, client_id);
    },
    Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
}
```

The interesting part here is what happens after the `Err(err)`. Let's break it down:

1. `Err(err) =>`: We're making the `Err` value that was inside the `Result`
   accessible in the variable `err`
2. `Err::<(), _>(err)`: We're wrapping the `err` back into a `Result` with the
   `Err()` function
3. `.with_context(err_context)`: We're attaching error context (the actual
   context isn't shown here)
4. `.non_fatal()`: We're logging the error with `non_fatal`

Notice that in step 2 we must qualify the final `Result` type, which is the job
of the `::<(), _>` attached to `Err`. This is necessary because at this point
Rust cannot determine on its own what the `Ok` value of the `Result` we create
may be. This isn't relevant for our case, because we *know* the `Result` will
never be an `Ok` variant, but Rust still requires this.

If you're getting errors about "type annotations needed" or "cannot infer type
for type parameter `T`" in one of your calls to `Err`, this is most likely
fixed by adding `::<(), _>`.


## Adding Concrete Errors, Handling Specific Errors

> **TL;DR**
> - Add a new variant to `zellij_utils::errors::ZellijError`
> - Use `anyhow::Error::downcast_ref::<ZellijError>()` to recover underlying errors

Sometimes you'll find yourself in a situation where you want to react to very
specific errors. For example, the "command panes" feature in zellij has a
special handling for "command not found" errors. If all the `anyhow::Error`s
are the same, how can we distinguish between underlying error types?

This is possible because while `anyhow` can unify a vast amount of errors into
the `anyhow::Error` type, it also gives us the possibility to recover
underlying error types. To do this, however, we must first know what error type
to expect.

External libraries, such as other crates or even `std` will likely define their
own error types. These error types have distinct error variants that one can
distinguish and react upon. But what happens, for example, if we create the
error we want to react to ourselves?

For this purpose, there is the `ZellijError`, which is contained in
`zellij_utils::errors`. It is built with the [`thiserror`][thiserror] crate and
hence easily extensible. If you need a specific error type to act upon, just
define a new variant in `ZellijError`. It is automatically available in any
source file that has the `use zellij_utils::errors::prelude::*;` statement in
it.

Once you have created your error instance, as soon as you wrap a `context`
around it, it is turned into an `anyhow::Error`. This makes it compatible with
all the other functions in the code that return `anyhow::Result`.

Recovering the error can look, for example, like this:

```rust
match pty
    .spawn_terminal(terminal_action, client_or_tab_index)
    .with_context(err_context)  // <-- Note how we attach a context, but can
                                //     still recover the error below!
{
    Ok(_) => {
        // ... Whatever
    },
    Err(err) => match err.downcast_ref::<ZellijError>() {
        Some(ZellijError::CommandNotFound { terminal_id, .. }) => {
            // Do something now that this error occured.
            // We can even access the values stored inside it, "terminal_id" in
            // this case
        },
        // You can check for other error variants here
        _ => {
            // Some other error, which we haven't checked for, occured here.
            // Now we can, for example, log it!
            Err::<(), _>(err).non_fatal(),
        },
    },
}
```



[tracking_issue]: https://github.com/zellij-org/zellij/issues/1753
[handle_panic]: https://docs.rs/zellij-utils/latest/zellij_utils/errors/fn.handle_panic.html
[miette]: https://crates.io/crates/miette
[anyhow]: https://crates.io/crates/anyhow
[thiserror]: https://crates.io/crates/thiserror
[context]: https://docs.rs/anyhow/latest/anyhow/trait.Context.html
[result]: https://doc.rust-lang.org/stable/std/result/enum.Result.html
[fatal]: https://docs.rs/zellij-utils/latest/zellij_utils/errors/trait.FatalError.html#tymethod.fatal
[non_fatal]: https://docs.rs/zellij-utils/latest/zellij_utils/errors/trait.FatalError.html#tymethod.non_fatal
[1]: https://github.com/zellij-org/zellij/commit/99e2bef8c68bd166cf89e90c8ffe8c02272ab4d3
[2]: https://doc.rust-lang.org/stable/std/io/enum.ErrorKind.html#variant.NotFound
[3]: https://github.com/zellij-org/zellij/issues/1753
[pr1881]: https://github.com/zellij-org/zellij/pull/1881
