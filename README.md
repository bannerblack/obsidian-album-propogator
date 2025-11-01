# Obsidian Album Propogator
![Obsidian base view](base.png)
![TUI screenshot](tui.png)

This project allows the user to search for their favorite artists, select albums, add them to a library, then pull album art and generate Obsidian notes with metadata from each of those albums.

Once albums are populated, the user can create a base and select the cover_art property to nicely showcase their favorite albums in an Obsidian base.

This applicaiton is rate limited to not put strain on MusicBrainz servers. After albums are added to the library, they are downloaded in the background as per the 1 request a second requirement. 

---

## Current Issues

- Erroneous network errors
- When downloading albums from the main searching function, the applciation attempts to target the first release. However, this occasionally causes album art failures. As a workaround, you can manually add album releases with CTRL-M.
    - This may be related to how the application stores release-group vs. release ids.

## Features to add
- Full Sveltekit + Tauri app, as the functionality is somewhat limited in TUI form.
- Better library control
- Custom templating
- Controlled save locations