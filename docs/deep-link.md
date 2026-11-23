# Opening an item from a launcher (deep link)

A launcher can open Native Jelly on one item's detail page by passing launch parameters:

```sh
luna-send -n 1 luna://com.webos.applicationManager/launch \
  '{"id":"com.sostk.nativejelly","params":{"itemId":"<Jellyfin item id>","serverId":"<Jellyfin server id>"}}'
```

- `itemId` is the item's Jellyfin id, dashed or not. `serverId` is optional; when it is given and
  names another server than the one signed in, the link is dropped.
- webOS passes `params` to a native app as one JSON string in `argv[1]`; `src/main.c` hands it to
  `app::deep_link`. Other keys (`tmdbId`, `mediaType`, `title`) are ignored.
- The link opens the **detail page**, never playback. It waits for Home (behind sign-in, the
  who's-watching picker or a first-run question) and is dropped after 45 seconds.
- Anything that is not a GUID is refused before it reaches the catalog or the log.
- The app declares `handlesRelaunch: false`, so a launch while it is running restarts it.

CleanYourTV's *Jellyfin continue watching* add-on sends exactly these parameters.
