# Image galleries

DDM downloads the pictures of a gallery (a Wikimedia Commons category, a Flickr album, a DeviantArt gallery, an Imgur album and many more) with the help of [gallery-dl](https://codeberg.org/mikf/gallery-dl), a free program that reads several hundred sites: it finds the gallery's pictures, and DDM downloads them. Like yt-dlp for videos, it's an optional helper: DDM installs it the first time you need it, or uses your own copy.

## Download a gallery

1. Click **Add URL** and paste the gallery's address, for example `https://commons.wikimedia.org/wiki/Category:Featured_pictures_of_birds`.
2. DDM opens **Download a gallery** instead of downloading the page as a file. The first time, click **Install the gallery helper**.
3. gallery-dl checks the address on your computer, without going online. When the line reads **gallery-dl can download this gallery's files**, you're ready.
4. Check **Save to** (the General category's folder by default). The pictures go into a folder of their own there, named after the gallery.
5. Optionally, type a number in **Files at most** to download only the first ones the site lists. On most sites those are the newest. Leave it empty for all of them.
6. Click **Download Now**, or **Later** to add it to the list without starting.

The gallery gets one row in the list. Its size grows as the pictures arrive, and its progress window says how many of the gallery's files are done ("12 of 40 files"). The log says how many were saved. Each picture is marked as downloaded from the internet, as any download is.

## How a gallery comes down

gallery-dl lists the gallery's files: where each one comes from, and the name it gives it in the gallery's folder. DDM's own downloader then brings them in, the way it downloads any file:

- **Several at a time**: as many as **Options → Connection** gives a file (8 by default). While many files are left, each comes over one connection; the last few share them. The progress window's connections table shows the files under way.
- **Under your speed limits**: one limit holds for the whole gallery, however many files come at once. It is the gallery's own, set in its progress window's **Speed Limiter** tab, or else the **Speed Limiter**'s limit for one file. The limit for all downloads together counts the gallery too. A change applies at once.
- **Paused and resumed** like a file, also after quitting DDM: a picture stopped half-way carries on from the bytes already saved.
- **Under gallery-dl's names**: each picture goes where gallery-dl would have saved it, so a gallery downloaded before by gallery-dl alone, or by DDM, finds its pictures and skips them.

A few files gallery-dl still downloads itself, after DDM's part, and the progress window says how many ("gallery-dl downloads 2 of them itself"):

- a video it hands to yt-dlp;
- a text it writes itself (a post's caption, for example);
- a file whose type only the server tells;
- a file the site refuses to DDM's downloader (one that wants gallery-dl's own sign-in, for example).

gallery-dl then downloads those its usual way, one a second at most, and skips the pictures DDM has saved.

## Get the new pictures later

Start a finished gallery again (**Resume**) to fetch only the pictures that aren't in its folder yet. The ones already there are skipped, whoever downloaded them.

## Stop, delete

- **Stop** pauses the gallery: the pictures saved so far stay, and a picture under way keeps what it had, for **Resume** to carry on from. gallery-dl ends with everything it started.
- **Delete** takes the gallery off the list. Its folder and pictures stay, even if you tick the box to delete files: delete the folder yourself if you want it gone.

## Which addresses open the gallery window

Add URL sends these to the gallery window:
- Wikimedia Commons categories;
- Flickr photographers and albums;
- DeviantArt and ArtStation galleries;
- Imgur albums;
- Pixiv artists and works;
- Pinterest boards and pins;
- Danbooru and Gelbooru searches;
- Tumblr blogs;
- Bluesky profiles.

A picture's own address (one ending in `.jpg` or `.png`) downloads as a file, as before.

## gallery-dl itself

- **Where it comes from:** DDM downloads gallery-dl from its releases on Codeberg. It accepts it only when the release's checksum list is signed by gallery-dl's author's key, pinned in DDM, and the program matches that list. It keeps the previous version and never goes back to an older one. On Windows and on Linux (x86-64) DDM installs it; on macOS, install it with Homebrew (`brew install gallery-dl`) or pip.
- **Your system's copy:** on Linux and macOS, DDM finds a gallery-dl on your `PATH` and in the usual package-manager folders, including Homebrew's, as it does yt-dlp. A program you chose yourself comes first, then DDM's own copy; the system's is used when neither is there, and your package manager keeps it up to date. If yours is too old for a site, **Use DDM's own copy** in **Helpers...** downloads DDM's (Windows and Linux x86-64), used from then on.
- **Your own copy:** **Helpers...** (in the gallery window or the video window) lists yt-dlp, ffmpeg and gallery-dl, and **Use another program...** takes your own copy of each instead of DDM's.
- **How it asks:** gallery-dl and DDM's downloader ask sites as DDM (with the User-Agent the browser handed over, if any), and name the gallery's site as the page the pictures come from, as a browser does. Wikimedia's servers refuse tools that ask without a name. What gallery-dl downloads itself, it downloads one file a second at most. It uses no configuration file of yours.
- **Network adapter:** while one is chosen (**Options → Proxy / Socks → [Network adapter](options.md#proxy--socks)**), galleries go through it too, as video pages do: DDM's downloader opens its connections through the adapter as for any file, and DDM gives gallery-dl a proxy of its own on this computer, opens every connection through the adapter and looks the names up with the DNS you chose. When the adapter disconnects, the gallery stops with the other downloads; nothing goes out through your normal connection.
- **The gallery-dl of your Linux distribution:** the one your distribution ships works too, from 1.20 (Ubuntu 22.04) on. Found on its own, it gets only the options every one of those versions knows; chosen in **Helpers...**, the ones its version knows. From 1.27.7 on, it lists the files for DDM's downloader; an older one downloads them all itself, as before, and the progress window says so. One found on its own is asked to list them all the same, and downloads them itself if it can't. One older than 1.20 (Debian 11's 1.17) is refused when you choose it, saying which version DDM needs; found on its own, its galleries fail, and **Use DDM's own copy** is the way out.

## On the command line

- `install-gallery-dl` installs or updates gallery-dl, verified in the same way.
- `gallery-get URL [-o folder] [--limit N] [-c N]` downloads a gallery into a folder named after it, `-c` files at once (8 by default). See [Command line and automation](command-line.md).
