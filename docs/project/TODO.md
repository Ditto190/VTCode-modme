implement /config options searchs functionality to allow users to quickly find and access specific configuration options within the TUI. Ensure the search is responsive and provides relevant results as the user types.

===

refine and improve config, models modal UI/UX for better user experience and clarity. use text styles, spacing, and visual hierarchy effectively. bold, dimmed, and highlighted text should be used to guide the user's attention and emphasize important elements. revamp and revise the layout, interactions, and feedback mechanisms to ensure intuitive and efficient user workflows.

currently it lacks a cohesive visual hierarchy and clear feedback mechanisms, making it harder for users to navigate and understand the available options. addressing these issues will significantly enhance the overall user experience and make the interface more intuitive and efficient.

to address these issues, consider implementing consistent spacing, clear labeling, and immediate visual feedback for user actions. use color, typography, and layout strategically to create a clear hierarchy and guide users through complex interactions. additionally, ensure that error messages and success indicators are prominent and easy to understand, reducing cognitive load and enhancing overall usability.

====

check large inputs to the chat input text field in the TUI. currently it shows the full text instead of previous content being truncated or summarized. implement a mechanism to handle long inputs gracefully. including text, images and file tokens handling for large pasting messages within the input.

====

check messages queue handling doesn't work properly. investigate the root cause of the issue and implement a reliable mechanism to ensure messages are queued, processed, and displayed correctly in the TUI. consider edge cases such as rapid message influx, large message sizes, and potential race conditions. when the main agent is busy or temporarily unavailable, messages should still be queued and processed once the agent is ready.
