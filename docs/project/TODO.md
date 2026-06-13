can we wrap the TUI container text for both edges of terminals? It looks weird when the text just abruptly ends at the edge without wrapping. Wrapping would make it look more polished and easier to read, especially for longer lines of text.

some components like quote, table are not wrapped properly and just get cut off at the edge of the terminal. Wrapping would ensure that all content is visible and the UI looks more polished. It would also improve readability, especially for users with smaller terminal windows.

---

audit vtcode-\* crates and check if can merged combines any similiar to reduce reduntdant.
