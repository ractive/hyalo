❯ We want to build a CLI tool to manage md files. The md files should be compatible with obsidian. We need to explore what this compatibility means. I don't want to
implement any functionality that manages obsidian or features of obsidian (like the vaults, file history, bookmarks, daily notes etc.), but keep compatibility of the md
files and also esp. of their frontmatter. Worth exploring:
- https://obsidian.md/help/obsidian-flavored-markdown
- https://obsidian.md/help/syntax
- https://obsidian.md/help/advanced-syntax
- https://obsidian.md/help/tags
- https://obsidian.md/help/callouts

Very important and useful are the properties that allow you to organize notes with structured data in the frontmatter. Allowed values are of type text, numbers,
checkboxes, dates, dates and times, or lists:
- https://obsidian.md/help/properties

The obsidian CLI has many commands to deal with the files like "create", "read", "append", which doesn't need to be implemented, because AI agents can do this obviously
out of the box. Interesting are the move and rename file commands that update the internal links of all the other md files (probably using the functionality to find all
outgoing and incoming links of a document (backlinks)).

Interesting commands are IMHO e.g: tags, tasks, properties, outline, links and search
Have a look here:
- https://obsidian.md/help/cli
- https://obsidian.md/help/plugins/search
- https://obsidian.md/help/plugins/outline
- https://obsidian.md/help/plugins/search

Esp. the search and the navigation by finding outgoing and incoming links could be very powerful tools for an AI agent to work with a bunch of md files.
To give an idea, what you can do with obsidian search:
```
# Find all ready stories (frontmatter query — much more reliable than grep)
obsidian vault=stova-knowledgebase search query='type = "story" status = "ready"' path="backlog" format=json

# Filter by priority
obsidian vault=stova-knowledgebase search query='type = "story" status = "ready" priority = "high"' path="backlog" format=json

# Filter by sprint (searches all domain subfolders)
obsidian vault=stova-knowledgebase search query='type = "story" sprint = "sprint-3" status = "ready"' path="backlog" format=json
```
How obsidian deals with frontmatter properties:
```
# Read a single property
obsidian vault=stova-knowledgebase property:read name=status path="backlog/backend/STORY-003-idx-parser-validator.md"

# Update story status — use this instead of editing the file manually
obsidian vault=stova-knowledgebase property:set name=status value=in-progress path="backlog/backend/STORY-003-idx-parser-validator.md"
obsidian vault=stova-knowledgebase property:set name=status value=done        path="backlog/backend/STORY-003-idx-parser-validator.md"

# Read all frontmatter properties
obsidian vault=stova-knowledgebase properties path="backlog/backend/STORY-003-idx-parser-validator.md" format=json
```
And this is how obsidian deals with tasks:
```
# Show task info
task file=Recipe line=8
task ref="Recipe.md:8"

# Toggle task completion
task ref="Recipe.md:8" toggle

# Toggle task in daily note
task daily line=3 toggle

# Set task status
task file=Recipe line=8 done      # → [x]
task file=Recipe line=8 todo      # → [ ]
task file=Recipe line=8 status=-  # → [-]
```

Keep this initial pitch already in the hyalo-knowledgebase so that we can iterate on it later.

Go through the documentation of obsidian to get a better picture and document the important things. Then let's discuss how we plan the iterations, what features make
sense, what is needed for which feature etc.
At a later stage, let's discuss how he can speed up searching documents based on properties etc. by introducing kind of an index.