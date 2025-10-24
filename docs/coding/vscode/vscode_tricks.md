# VsCode Tips

## LLM usage tips

As of 2025-10-24, VsCode coding agent options.
>GPT-5 : the cleverest heavyweight model, high context window. Expensive. Slow. Use it for hard tasks when faster models are confused.
>GPT-5-Codex: faster, smaller, cheaper version of GTP-5. More trained for coding. Problems on long tasks.
>Grok-Code-Fast 1: very fast (3x faster) and cheap model. Excellent for easy tasks. When it matters that if you have to wait 20 seconds or 60 seconds for a job.

Suggestion: **for speed, use Grok-Code-Fast 1** first. It is usually OK. If it makes a mistake or it is a **difficult task, switch to GTP-5**.

>There is also Amazon Q, which is free, but probably it cannot go Agentic workflow

## Notes
- start vscode in any directory in CMD, type: "code ."  // runs code with the current directory.

- Better way (than GitHub diff) to show the Diff of previous changes in the Timeline.
"If you haven't seen Visual Studio Code for the Web in action, you can press '.' (the period key) in the <> Code tab of a GitHub repository 
and you will launch a web-based version of VS Code to browse and edit the source code."
>Debug doesn't work of course, but in theory changes can be made and Commit can be made to GitHub.

Menu: "File / Auto Save" toggle that turns on and off save after a delay (1000ms). Never have to press Ctrl-S again. Worth switching on, so we don't have to check that everything is saved before compiling.

Shortcuts:

IntelliSense:
- Ctrl+Space has been the predominant keybinding to trigger IntelliSense (Code completion). But it is triggering auto, when '.' is pressed. https://code.visualstudio.com/docs/editor/intellisense
(If a language service (JavaScript, JSON, HTML, CSS, SCSS, Less, C# and TypeScript) knows possible completions, the IntelliSense suggestions will pop up as you type. You can always manually trigger it with Ctrl+Space.)
(By default, Tab or Enter are the accept keyboard triggers)

Debugger:
- F5: Debug, attach debugger
- Ctrl-F5: Run without attaching debugger

Code editing:
- Select word + Ctrl-F3/ Ctrl-V / F3 / Ctrl-V / F3 ... combo is immensely powerful to replace words to another word 20 times in the file.
- After Ctrl-F (Find in file): 'Enter' and 'Shift+Enter' to navigate to next or previous result
- F12: Go to definition
- Go to definition via mouse: hold down Ctrl, Move mouse over a method, simple click (not double click). After that Ctrl-Back will go back.
- Alt-LeftArrow, Alt-RightArrow: Navigate back and forward. (after Go To Definition, or after changing opened files.). Navigate Forward only makes sense After a Navigate Back, otherwise nothing happens.
- F2: Refactoring:Rename symbol (function, variable) in all files. see https://code.visualstudio.com/docs/editor/refactoring
- Ctrl-K, Ctrl-F Format selection.

- Ctrl+Shift+O (Go to Symbol, in editor window). type : for lines, @ for symbols. (no local symbols, only class symbols in C#, in TS it works for local symbols too) Can save a lot of navigation time.
- Ctrl+T (Go to Symbol, in workspace) (jumping to a symbol across files)
- Whole block-selection easily with keyboard: Put cursor in a block somewhere. Expand and shrink selection  Expand: Shift+Alt+RightArrow, Shrink: Shift+Alt+LeftArrow

- remove "/// <summary>" and "/// </summary> lines from the code.
    With RegEx replacement: ^(\s)+///\s<summary>$\n => "\n" and ^(\s)+///\s</summary>$\n => ""  (VsCode) 
        or ^(\s)+///\s<(/)*summary>$\r\n (Notepad++)
    Explanation: ^: beginning of the line, $: end of the line, \s: any whitespace
    Then replace "/// " to "// " (without RegEx)

Multiple cursors, multiple selections: (See Selection menu)
- Alt+Click : Add new cursors anywhere you click
- Ctrl+D: ('Add Next Occurence') selects the word at the cursor (with single cursor). Pressing again create 1 new cursor selecting the same word again.
- Ctrl+Shift+L: ('Select All Occurences') create many new cursors selecting the same word again. (that is visible on screen, no point creating cursors which are not visible.)

Vscode services:
- Ctrl+Shift+P: Command Palette (">" appears, which can be removed by Del): start searching filenames to load in or start typing commands. Then Enter if there is only one possible command or Tab + Down to select command (without mouse)
- Ctrl+P (Quick Open 'file') + and start to type any filename: "*.ts", or "Program.cs". It is easier to find files than browsing. You can now continue to navigate to the symbols of a file result simply by typing '@'.

- (Ctrl + ,): Search Settings
- Ctrl + F4: close tab page window
- (Ctrl + ') toggles the Terminal window parts
- (Ctrl+Shift+B) start build