# Project Architecture

## Modular Structure

The Zellij project is structured as a collection of Rust crates, each serving a distinct purpose in the application's functionality. The main crates include:

- **zellij-client**: Handles the client-side logic, managing interactions with the user interface and sending commands to the server.
- **zellij-server**: Manages the backend logic, handling connections from clients, maintaining sessions, and coordinating the overall runtime environment.
- **zellij-tile**: Provides the logic for individual UI tiles, allowing for custom plugins and layouts within the terminal interface.
- **zellij-tile-utils**: Contains utility functions and shared logic used by the tile-related crates.
- **zellij-utils**: Houses general-purpose utility functions and structures used throughout the project.

These crates are interconnected to form a cohesive system where the client communicates with the server to manage sessions, while tiles provide a flexible and extensible UI layer.

## Interaction Between Components

- The **zellij-client** communicates with the **zellij-server** over a network connection, sending user inputs and receiving updates on session states.
- The **zellij-server** manages the lifecycle of sessions and distributes commands to the appropriate components.
- **zellij-tile** modules can be dynamically loaded and managed, providing an extensible UI framework.
- Shared utilities in **zellij-utils** and **zellij-tile-utils** ensure consistency and reduce duplication of common logic.

This modular design allows for independent development and testing of individual components, making the system highly maintainable and scalable.