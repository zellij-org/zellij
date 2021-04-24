This document details the API offered by the server to clients. Each method description contains:
- The method's name
- The method's purpose and intended effect on the server and/or the appearance of the screen rendered by zellij.
- The method's parameters
- The response that the client can expect to receive, including any possible error messages. N.B. Response does not detail side-effects of the request, in particular changes in what's rendered on screen.

The document does not describe implementation details on the server side and is intended for use by those implementing new zellij clients.

## Terminal controls

### New pane

**Purpose:** Opens a new [pane](./TERMINOLOGY.md#pane).

**Parameters:** Direction (optional) - the position of the new pane relative to the focus pane.

**Response:** Done opening new pane - indicates that the server has finished opening the pane. This exists for the purpose of synchronization @@@what's blocked?

## Close pane

**Purpose:** Close the focus [pane](./TERMINOLOGY.md#pane).

**Parameters:** None.

**Response:** Finished opening new pane - indicates that the server has finished closing the pane. This exists for the purpose of synchronization @@@what's blocked?

## Switch focus pane

**Purpose:** Switch the focus [pane](./TERMINOLOGY.md#pane). The focus pane is the target for many other pane-related actions.

**Parameters:** @@@none?

**Response:** None.

## Move focus pane

**Purpose:**

**Parameters:** 

**Response:** 

## Toggle fullscreen focus pane

**Purpose:** In a normal layout, switches the focus [pane](./TERMINOLOGY.md#pane) to be fullscreen. When the focus pane is fullscreen, switches back to the normal layout, potentially with multiple panes.

**Parameters:** None.

**Response:** Whether the layout has the focus pane locked to fullscreen or not.

## Resize focus pane

**Purpose:** Resizes the focus [pane](./TERMINOLOGY.md#pane). 

**Parameters:** 

**Response:** 

## Session management

**Name:** 

**Purpose:** 

**Parameters:**

**Response:**
