https://huggingface.co/docs/hub/agents-overview#register-your-agent-harness

===

implement automatically download any new updates when vtcode starts up.

---

CRITICAL: check session logs:

error:

1. The apply_patch tool is also being routed to unified_search
2. This appears to be a test/sandbox environment where the tools simulate behavior but don't actually write to disk.
3. why defuddle_search while this is file search only?
4. check the logs and find out why the apply_patch tool is being routed to unified_search instead of file search. This may be a misconfiguration or a bug in the routing logic.
5. it's unusable.
6. Double check for any misconfigurations in the tool routing settings and ensure that the apply_patch tool is correctly associated with the file search functionality. If necessary, review the codebase for any recent changes that may have affected the routing behavior.
7. Check for redundant or conflicting tool definitions that could be causing the routing issue. Ensure that the apply_patch tool is not inadvertently being overridden by another tool with similar functionality.
8. Check for wasted context in the logs and ensure that the routing logic is optimized for efficiency. Remove any unnecessary steps or redundant checks that may be causing delays or misrouting of tools.
9. Check for error handling and logging mechanisms to ensure that any issues with tool routing are properly captured and reported. Implement additional logging if necessary to help diagnose the problem.
10. Figure out why build mode is not working and why the apply_patch tool is being routed to unified_search instead of file search. This may require a deeper investigation into the build process and any recent changes that may have affected the routing behavior.
11. Check tool policy permission and ensure that the apply_patch tool has the necessary permissions to be routed to the correct functionality. Review any access control settings that may be affecting the routing behavior.

log: /Users/vinhnguyenxuan/Documents/podcast/.vtcode/checkpoints/turn_1.json

review the logs and identify any patterns or recurring issues that may be contributing to the routing problem. Look for any error messages, warnings, or other indicators that could provide insight into the root cause of the issue. /goal Perform a comprehensive code review and debugging process by following these structured steps:  1. Code Audit and Prioritization: Analyze the provided code to identify bugs and technical debt. Categorize each finding by severity: Critical, High, Medium, or Low. Filter out any false positives to ensure the analysis is accurate. 2. Consolidated Reporting: Generate a single, organized report that summarizes all identified issues and provides specific, actionable recommendations for improvement. 3. Refactoring Analysis: Identify opportunities to optimize the codebase by applying KISS (Keep It Simple, Stupid) and DRY (Don't Repeat Yourself) principles. 4. Implementation and Testing: Execute the necessary fixes and refactorings. Conduct thorough testing to verify that all issues are resolved and that no regressions have been introduced. Note: focus on main code, not on test code. 5. Analyze the specific module for brittleleness and tight coupling. Break apart this monolith into smaller, independently testable chunks. implement strict interface guard rails to isolate this logic for the next generation phase. ==> after review done, you need to fix it and verify again until all done.

Note: For all coding tasks use your judgement to decide an appropriate lower power model and run that in a subagent

---

check and improve the first time lauch onboarding, improve the error message for guidance settings API to .env or just show a API key secured input box in the onboarding flow. whenever user hit error like this:

```

  ////////////////////////////////////////////////////// Error //////////////////////////////////////////////////////
    LLM request failed: Unauthorized
  ///////////////////////////////////////////////////////////////////////////////////////////////////////////////////
  ------------------------------------------------------ Info -------------------------------------------------------
    Hint: Verify your API key or credentials; Check that your account is active and has sufficient permissions;
    Ensure environment variables for API keys are set correctly
  -------------------------------------------------------------------------------------------------------------------
```

check existing /model selecting flow and improve it to be more user friendly and easier to setup API keys and model selection. Consider adding a guided setup wizard that walks users through the process of entering their API keys, selecting models, and configuring settings. Provide clear instructions and tooltips to help users understand each step.
