This document details the API offered by the server to clients. Each method description contains:
- The method's name
- The method's purpose and intended effect on the server and/or the appearance of the screen rendered by zellij.
- The method's parameters
- The response that the client can expect to receive, including any possible error messages. N.B. Response does not detail side-effects of the request, in particular changes in what's rendered on screen.

The document does not describe implementation details on the server side and is intended for use by those implementing new zellij clients.


## Basic controls

### Write input

**Purpose:** Writes a set of bytes to the terminal, not to be interpeted by zellij.

**Parameters:** Vector of bytes to be written.

**Response:** None.

### Quit

**Purpose:** Indicates that the current client is terminating.

**Parameters:** None.

**Response:** None.

### Switch input mode
@@@Is this part of the API or a client detail?

**Purpose:** Switches between different zellij [input modes](./TERMINOLOGY.md#input_modes).

**Parameters:** Which input mode to switch to.

**Response:** None.

### Scroll up

**Purpose:** Scrolls up (back) in the screen buffer of the focus [pane](./TERMINOLOGY.md#pane).

**Parameters:** None.

**Response:** None.

### Scroll down

**Purpose:** Scrolls down (forwards) in the screen buffer of the focus [pane](./TERMINOLOGY.md#pane).

**Parameters:** None.

**Response:** None.


## Pane controls

### New pane

**Purpose:** Opens a new [pane](./TERMINOLOGY.md#pane).

**Parameters:** Direction (optional) - the position of the new pane relative to the focus pane.

**Response:** Done opening new pane - indicates that the server has finished opening the pane. This exists for the purpose of synchronization @@@what's blocked?

### Close pane

**Purpose:** Close the focus [pane](./TERMINOLOGY.md#pane).

**Parameters:** None.

**Response:** Finished opening new pane - indicates that the server has finished closing the pane. This exists for the purpose of synchronization @@@what's blocked?

### Switch focus pane

**Purpose:** Switch the focus [pane](./TERMINOLOGY.md#pane). The focus pane is the target for many other pane-related actions.

**Parameters:** @@@none?

**Response:** None.

### Move focus pane

**Purpose:**

**Parameters:** 

**Response:** 

### Toggle fullscreen focus pane

**Purpose:** In a normal layout, switches the focus [pane](./TERMINOLOGY.md#pane) to be fullscreen. When the focus pane is fullscreen, switches back to the normal layout, potentially with multiple panes.

**Parameters:** None.

**Response:** Whether the layout has the focus pane locked to fullscreen or not.

### Resize focus pane

**Purpose:** Resizes the focus [pane](./TERMINOLOGY.md#pane). 

**Parameters:** Direction in which the pane should be resized. @@@ When does it shrink, when does it grow? Is it the top left that's fixed?

**Response:** None.


## Tab controls

### New tab

**Purpose:** Opens a new [tab](./TERMINOLOGY.md#tab).

**Parameters:** None.

**Response:** None.

### Close tab

**Purpose:** Closes the current [tab](./TERMINOLOGY.md#tab).

**Parameters:** None.

**Response:** None.

### Next tab

**Purpose:** Switches to the next [tab](./TERMINOLOGY.md#tab) by index.

**Parameters:** None.

**Response:** None.

### Previous tab

**Purpose:** Switches to the previous [tab](./TERMINOLOGY.md#tab) by index.

**Parameters:** None.

**Response:** None.

### Go to tab

**Purpose:** Switches to a specific [tab](./TERMINOLOGY.md#tab).

**Parameters:** Index - the index of the tab within the list. @@@ Shouldn't this really be based on name? Index is quite internal.

**Response:** None.

### Last used tab

**Purpose:** Switches to the last [tab](./TERMINOLOGY.md#tab) that was in use.

**Parameters:** None.

**Response:** None.

### Rename tab

**Purpose:** Renames the current [tab](./TERMINOLOGY.md#tab).

**Parameters:** A vector of bytes containing the new name for the tab.

**Response:** None.

## Session management
