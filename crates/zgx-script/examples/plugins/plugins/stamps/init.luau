-- Stamps: a mark before the title (adding is open), a friendlier empty
-- state in place of the app's (replacing takes Override("#empty")), and
-- each row wrapped with a "done" badge by replacing the app's row module
-- (Override("@app/components/task_row"), a trust grant: the replacement
-- runs as the app's own code).
ui.contribute { target = "#title", before = ui.text { id = "stamp", "✓" } }
ui.contribute { target = "#empty", replace = ui.text { id = "all-clear", "All clear: add a task above." } }
ui.override { module = "@app/components/task_row", with = "./task_row" }
