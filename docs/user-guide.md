---
title: Audiobook Organizer - User Guide
date: 2026-07-05
status: living document
---

# Audiobook Organizer - User Guide

## What this is, and who it is for

Audiobook Organizer is a calm desktop helper for a messy audiobook collection. Over the years a big library picks up clutter:

- loose files sitting on their own instead of in folders,
- folders with odd names full of leftover download tags,
- box sets with several books crammed into a single folder,
- and the occasional accidental duplicate.

This tool looks at your collection, explains in plain words what is untidy, and proposes a gentle reorganization so that Audiobookshelf (the app that plays your books) can read authors, series, and years correctly.

It is built for everyone in the house, not just the person who set it up. If you can read a shopping list and press a button, you can use it. You will never be asked to understand a file path, a command, or a technical term in order to review or approve a tidy-up.

Here is the thinking behind that. The people who move hundreds of gigabytes of hard-to-replace books are exactly the people who feel nervous doing it. Rather than hide that worry, the tool is designed around it: you always see what would happen before anything happens, and anything that does happen can be undone.

A few things it is deliberately not:

- It does not play your audiobooks. Audiobookshelf does that.
- It does not replace Audiobookshelf. It gets your shelves ready for it.
- It does not go looking things up on the internet. Everything it knows comes from your own files.

In one line: it is a tidier, not a player and not a downloader.

## The promise

Here is the sentence the whole tool is built to keep, word for word, and it appears the same way everywhere you will see it:

> No audiobook is ever deleted. Only empty folders are removed, and every change can be undone.

That is not marketing. It is the design. Three things make the promise real, and each of them is a part of the product, not an afterthought bolted on.

**The preview comes first.** Before a single file moves, the tool does a "dry run". It works out the whole plan and shows it to you, both on screen and as a report you can read anywhere. Nothing is touched while you read. You look, you decide, and only then does anything move. This rhythm has a name inside the tool: scan, then review, then confirm. No step is ever skipped, and the moving part comes last.

**The report is yours to keep.** The dry run produces a self-contained web page you can open, read at the kitchen table, print, or set aside to read tomorrow. It lists every proposed change in plain language. It is the thing that lets you say, with confidence, "yes, this is what I want" or "no, not that group" before you commit to anything.

**Undo is always there.** As the tidy-up runs, the tool writes down every change the moment it makes it, so the whole thing can be reversed afterward. Two details make this trustworthy:

- Duplicate copies are never deleted. They are moved to a holding folder that you empty yourself, when you are ready.
- Empty folders (folders with no audio in them at all) are the only thing ever removed, and even that can be undone.

The point of all three is the same. You should be able to press the button on your whole collection without fear.

## How a tidy-up works

A tidy-up always follows the same calm rhythm.

**1. Scan.** You point the tool at your library folder once. It reads through everything (only reads, never changes) and builds a quiet picture of what is there: how many books, how many need attention, and why. A scan of a large library takes under a minute on a normal drive. You can stop it at any time, and because a scan only reads, stopping is always perfectly safe. Nothing is at risk during a scan.

**2. Review.** The tool sorts everything it wants to do into seven groups. Each group is one kind of tidying, described in plain words with a few real examples pulled from your own shelves. You turn groups on or off with a simple switch. Nothing is decided for you, and you can include some groups while skipping others.

**3. Confirm.** When you are happy, you press the one clear button. It does not throw up a scary dialog box. It simply asks, right there in place, "ready?" with a plain "go ahead" and a quiet "not yet". You are always one calm step away from stopping.

**4. Tidy.** Now the changes happen, in order, with a running list of what is being moved so you can watch it work. You can pause between books, or stop at a safe point. Every change is written down the instant it happens, which is what makes the next step possible.

**5. Undo.** Afterward you get a summary and an "undo" button that stays available. If anything looks wrong, or you simply change your mind, one press puts it all back exactly as it was.

### The seven groups

Every tidy-up is built from these seven groups, and only these seven. You can include or skip any of them, and the same seven appear in the app, in the review, and in the report, always with the same names and in the same order, so nothing ever surprises you.

**1. Sorting piles (staging).** Many people keep a "to sort" or "in progress" folder inside their library while they are still organizing. Audiobookshelf gets confused by those and treats half-sorted piles as if they were finished books. This group moves your sorting piles out of the library so only finished books remain.

- Example: a folder called `_sort` sitting among your books gets lifted out to the side, with everything inside it kept exactly as it was.

**2. Loose books.** A book that is just a single file sitting loose among your folders is hard for Audiobookshelf to shelve properly. This group gives each loose book its own tidy folder.

- Example: a stray file named `Sapiens by Yuval Noah Harari.m4b` becomes a proper folder, `Sapiens`, with the book tucked neatly inside it.

**3. Messy names.** Downloaded books often arrive with clutter in the folder name: bracketed tags, bitrate and size markers, ranking numbers, underscores standing in for spaces. This group cleans the names up to something a person would actually write, and it also fixes series numbering so that book two reads as book two. Nothing about the audio changes, only the label on the folder.

- Example: `[AudioRip] 03_the-way-of-kings_320kbps` becomes simply `The Way of Kings`.

**4. Box sets.** Sometimes one folder holds several complete books side by side, and Audiobookshelf collapses the whole thing into a single wrong item. This group splits those folders so each book stands on its own.

- Example: a single `Harry Potter Complete` folder holding seven books becomes seven folders, one per book, each named for its own title.

**5. Bundles.** Collection packs (an award set like the Hugo winners, or a themed bundle) nest real books several layers deep, which hides them from Audiobookshelf. This group lifts each book out onto the shelf where it belongs. Crucially, the fact that a book came from the Hugo pack is not thrown away. It is written into a saved report so you never lose that history (more on this below).

- Example: a `Hugo Winners` pack with a dozen novels buried inside becomes a dozen books on the shelf, plus a report line noting each one was a Hugo winner.

**6. Copies.** If the same book exists twice, this group sets the extra copy aside so you are left with one clean shelf entry. Set aside means moved to a holding folder, never deleted. Before it moves anything, the tool double-checks that the copies really are the same book, so you never end up with two folders for one book by accident.

- Example: two identical copies of the same novel become one copy on the shelf and one copy waiting in the Set Aside folder for you to review.

**7. Empty folders.** After everything else settles, some folders are left with no audio in them at all. This group sweeps those empty shells away. These are the only things the tool ever removes, and even that can be undone.

- Example: an old folder that once held a book, now empty because the book moved to its proper home, is quietly cleared.

### If something interrupts a tidy-up

Because real computers hiccup, the tool is built to fail safely and say so in plain words. You do not need to know how it works, only that it is looking after you:

- If a scan stops early, you are told plainly, and reminded that a scan only reads, so nothing was changed. You can just try the scan again.
- If a tidy-up stops partway (a power cut, an app closed, a folder the computer would not let it touch), everything done up to that point is already saved and can be undone. The tool names the book it stopped on and offers to carry on or to put things back.
- The tool works on one tidy-up at a time and never leaves your library in a half-known state. At worst a single change is in doubt, and the tool sorts that out for you when it starts up again.

The theme throughout is the same: when the tool is unsure, it stops and asks, rather than guessing.

## Reading your dry run report

The dry run report is a single web page. It opens on its own, needs no internet, and reads top to bottom like a short letter about your library. Because it is just one self-contained file, you can:

- read it on screen before you decide anything,
- print it and read it away from the computer,
- save it to look at later, or send it to someone whose opinion you trust,
- keep it as a record of what a tidy-up was going to do.

Here is what each part of the report is telling you, in the order you will meet it.

**The top.** A title, the date and time the report was made, and a plain line naming your library and the shelf layout it would use. This is how you know you are looking at the right, current report.

**The opening paragraph.** A few sentences that say, in bold, that nothing has been changed yet, how many files were read, how many changes were worked out, and that they fall into the seven groups. It is the report reassuring you before it shows you any detail.

**The seven groups at a glance.** A small table with one row per group: what it would do, how many changes, how much it involves, and a status. This is your map of the whole tidy-up on one screen.

**Before and after examples.** For each group, a handful of real examples from your own shelves, showing the messy "now" and the tidy "after" side by side. Clutter that gets removed from a name is shown crossed out so you can see exactly what goes. A short line ("and 234 more like these") points you to the full list at the end.

**Books that need a decision.** A highlighted section listing any book the tool will not touch until you choose. It names each one and says plainly that it stays exactly as it is until you decide. For instance, if a book exists as two different versions that would both want the same folder, the report tells you and waits, rather than picking one for you.

**What will not happen.** The guarantee, spelled out so there is no doubt:

- no audiobook is ever deleted,
- duplicate copies move to a holding folder beside your library and wait there until you empty it,
- no audio is edited (books move and folders are renamed, but the sound inside your files is never touched),
- an undo record is written as changes happen, so the whole tidy-up can be reversed,
- and nothing ever leaves your computer.

**Where books came from (provenance).** A list of every pack or award collection whose books were unpacked, and which books belonged to it. This is how the Hugo or Nebula history survives the tidy-up.

**The complete list.** At the very end, every single proposed change, grouped and listed in full, so the record is complete. You never have to take the summary on faith.

Three words show up in the statuses, and they each mean something specific and calm:

- **Checking** means a group (almost always the copies group) is still confirming that files really are identical before anything is set aside. It is being careful, not stuck.
- **Later** means you chose to skip that group this time. Nothing in it will be touched now; it simply waits for a future tidy-up.
- **Needs your eyes** means the tool found something it will not guess about (an unclear name, a book that exists as two different versions, a folder that is mostly video or a course rather than a book). It hands those to you rather than deciding for you. For example: "The War of Art has a v1 and a v2 that would land in the same folder; pick a keeper, or keep both as editions." The tool did not fail. It is deferring to you.

## Where your book names come from

This matters, so here it is plainly. Every title, author, series, and year the tool proposes comes entirely from your own collection. There are two sources, and only two.

**First, your folder and file names.** The way you (or whoever you got the books from) named things is the main source. In this kind of library, folder names turn out to be more reliable than anything else, so they lead. If a folder is called `Brandon Sanderson - Mistborn`, that is what the tool goes by.

**Second, a one-time local check of the labels already inside your audio files.** Audio files can carry their own title and author information written into them, the same information a music player shows. The tool read a sample of those labels, once, from files already on your disk, purely to double-check that your folder names really are the more reliable source (they are; the folder names won clearly). That check never changed your files, and today every proposed name still comes from the folder and file names alone. Using the inside-the-file labels to fill gaps is a possible later refinement, and if it ever arrives it will be the same kind of read: local, look-only, never editing anything.

That is the whole list. Two sources, both already on your own computer. In particular:

**The tool never goes online to identify a book.** It does not consult Audible, Goodreads, Open Library, or any other internet database. It does not search the web. It does not "phone home". There is no online lookup anywhere in it.

**It cannot reach the internet even if it wanted to.** This is not a promise on trust alone. The program is built with the network doors bolted shut, and there is an automatic check that fails the build if anyone ever adds code that tries to reach out. Zero network requests is a rule the tool is not allowed to break. Everything stays on your computer.

**Award and collection history is preserved, never lost.** When the tool unpacks a bundle (say, the Hugo winners set) onto your shelves, the fact that a book belonged to that collection is written into the saved report described above. The books move to where Audiobookshelf can read them, and the "this was a Hugo winner" knowledge is kept safe in the report. Nothing about that history is thrown away.

## What the tool will never do

- Never delete an audiobook. The only thing ever removed is an empty folder, and that can be undone.
- Never touch a file before you have previewed the plan and confirmed it.
- Never reach the internet, look a book up online, or send any information anywhere. No usage tracking, no crash reports, no cloud.
- Never edit the audio inside your files, and never rewrite the labels inside them.
- Never make changes inside Audiobookshelf on your behalf.
- Never guess about a book it is unsure of. Unclear names, two-version conflicts, and folders that are really video or a course are handed to you, not auto-organized.
- Never overwrite one file with another, and never follow shortcuts or links out to other parts of your disk.
- Never run a real tidy-up without a backup decision from you first (see below).

## Today, and what is coming soon

An honest picture of where the tool is right now.

**Working today** is the thinking part and the report. The tool can already:

- scan a real library and understand it,
- work out a complete, careful tidy-up plan,
- and produce a trustworthy dry run report you can read, print, and share.

That is the engine and the trust artifact, and they are done.

**Coming next** is the clickable app. The friendly on-screen version, with the shelves of covers, the seven review cards, and the switches described in this guide, is the next piece being built. Today the plan and the report exist; the window you click through is arriving after them.

**Coming after that** is the actual tidying. Moving files, setting copies aside, and the undo that reverses it all come once the app is in place. And that step will be proven on practice copies of real libraries first, with the undo shown to put everything back exactly as it was, before it is ever offered for a real collection.

The order is deliberate:

- understand the library first,
- let you see and approve a plan second,
- and only then move anything, with the safety proven before your trust is asked.

So if you are reading a dry run report today, that is exactly the intended experience for this stage. The report is real and complete. The one-button tidy-up it describes is being built toward, carefully, in that order. Nothing about the promise changes as the moving parts arrive; they are simply held back until they can keep it.

## Your choices: backing up first

Before any real tidy-up runs, the tool asks you to decide how you want to be protected first. This is your call, not the tool's, and there is no wrong answer. The tool simply will not run a real tidy-up until you have chosen. Three options, laid out neutrally:

- **Copy to another drive.** Make a full copy of your library onto a separate external drive before tidying. This is the most protective and needs the most space and time.
- **Copy on the same drive.** Make a second copy on the same disk. Faster and simpler, though it does not protect you if the whole drive fails.
- **Rely on the undo record and the set-aside folder.** Trust the tool's own safety net: the undo record that can reverse every change, and the fact that copies are set aside rather than deleted. This uses no extra space, and leans entirely on the built-in reversibility.

Pick the one that matches how cautious you feel and how much room you have. The tool records your choice and proceeds only after you make it.

A short, calm checklist before any real tidy-up:

- Read the dry run report and make sure it describes what you actually want.
- Do a fresh scan if it has been a while, so the plan matches your library as it is today.
- Choose your backup option above.

With those three done, you can press the button knowing exactly what will happen and knowing it can be undone.

## Where your files live

The tool keeps everything it produces in plain, findable places on your own computer. There are two, and both belong to you.

**The Reports folder.** Every report and record the tool makes lands here as an ordinary file:

- your dry run reports,
- the record of what a tidy-up actually did,
- the after-the-fact check that confirms everything landed where it should,
- the report of pack and award history (which book came from which collection),
- and any duplicate summaries.

Because these are just ordinary files, you can open them any time, keep them, print them, or delete them yourself. The tool never hides its records from you.

**The Set Aside folder.** When the copies group sets a duplicate aside, the copy moves here, into a folder named "Set Aside" that sits beside your library, not inside it. A few things are worth knowing:

- Nothing in this folder is ever deleted by the tool.
- It waits there until you look through it and empty it yourself, whenever you are ready.
- It sits outside your library so Audiobookshelf never mistakes a set-aside copy for a real shelf book.

It is your safety holding area, entirely under your control.

That is the whole shape of it: the tool reads, explains, previews, waits for your yes, moves carefully, and can always undo. Everything stays on your computer, nothing is ever deleted out from under you, and you are always the one who decides.

---

## For the curious

The plain names in this guide map to internal labels used in the project's own documents, in case you ever go looking. This mapping is optional reading and changes nothing about how the tool works.

- The exact promise ("No audiobook is ever deleted...") is the deletion guarantee, internally FD-10.
- Zero internet access is internally FD-11 (fonts bundled, zero network) plus the no-telemetry rule.
- Your backup being your own choice is decision D-17.
- The seven groups are the campaign group canon, internally FD-26.
- The dry run report's format is specified as F-506.
- Reading the labels inside your files, once and read-only, relates to the tag-quality probe (FD-14) and cover reading (F-907).
- Preserving pack and award history in the report is the provenance feature, F-507.
- "Set aside" rather than "delete" is the quarantine-only safety invariant (D-09); the on-disk folder name "Set Aside" is fixed by FD-31.
