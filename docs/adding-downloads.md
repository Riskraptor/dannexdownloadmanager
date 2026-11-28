# Adding downloads

[← Back to docs index](README.md)

There are several ways to give DDM something to download. Most of them end in the same window, **Download File Info**, where you choose where the file goes.

## Add URL

1. Copy the file's address (in a browser: right-click the link → **Copy link**).
2. Click **Add URL** on the toolbar, choose **Tasks → Add new download**, or press **Ctrl+N**.
3. **Add a download** opens. If the clipboard holds a web address, it is already filled in. The arrow at the end of **Address** lists the last 20 addresses you entered; pick one to use it again. Right-click the box for **Cut**, **Copy**, **Paste**, **Delete** and **Select all**.
4. If the server needs a user name and password, tick **Log in to the server** and type them.
5. Click **OK**.

DDM takes `http://`, `https://`, `ftp://`, `ftps://`, `ftpes://` and `magnet:` addresses. Anything else gets **Invalid address**.

![Add a download, with the Address box, Log in to the server and OK](images/help/add-url.webp)
*The numbers are the steps above: 3 the address, 4 a login if the server needs one, 5 OK.*

If the address is a video's page (YouTube, Vimeo, TikTok and others), the video window opens instead. See [Video downloads](video.md). A magnet link opens **Add torrent**; see [Torrents](torrents.md). A Metalink's address (`.metalink`, `.meta4`) is read first; see [Metalinks](#metalinks).

## FTP and FTPS

FTP downloads work like web downloads: several connections at once, pause and resume, and the
same progress window.

- `ftp://` — plain FTP.
- `ftpes://` — FTP secured with TLS from the start of the session (explicit FTPS, `AUTH TLS`, usually port 21). Use it when the server says it supports TLS.
- `ftps://` — implicit FTPS, port 990 unless the address gives another.

The server's certificate is checked against Windows' (or the system's) trusted certificates, as
for `https://`. A Linux system without them (a minimal install without the `ca-certificates`
package) still downloads over plain HTTP and FTP; each secure download then fails at once, saying
to install that package.

**Logging in.** Write the login in the address (`ftp://user:password@server/folder/file.iso`),
tick **Log in to the server**, or save it in **Options → Sites Logins**. Without one, DDM logs in as
`anonymous`. The path is relative to the folder the login starts in; to start from the server's
root, write `%2F` after the server (`ftp://server/%2Fpub/file.iso`).

**Resuming.** DDM keeps the file's size and modification time. If either has changed when you
resume, it says the file changed on the server instead of mixing two versions
(**Options → Downloads → Resume even if the file's date on the server changed** skips the time check). The
password is not written to the resume file next to the download: DDM gives it again when it
resumes.

**Connections.** Passive mode is used by default. **Options → Proxy / Socks → Use passive mode
for FTP** switches it off for servers that only take active connections. FTP connects to the
server directly, not through the HTTP proxy.

Browsers no longer download `ftp://` links themselves. Right-click one and choose **Download with
Dannex Download Manager**, or copy it and use **Add URL**.

## Google Drive links

Paste a Google Drive link as it is, the one **Share → Copy link** gives:

- `https://drive.google.com/file/d/…/view?usp=sharing`, with or without `/view`;
- `https://drive.google.com/uc?id=…&export=download` and `https://drive.google.com/open?id=…`;
- `https://docs.google.com/document/d/…/edit` (and Sheets, Slides, Drawings).

DDM downloads the file itself, as it was uploaded (a video comes in its own format, not the
player's copy), with its real name and size, over several connections, and it can resume. For a
large file, Drive shows a page that says it **can't scan this file for viruses**; DDM goes past
it as **Download anyway** does.

**Google Docs, Sheets and Slides** come as Word (`.docx`), Excel (`.xlsx`) and PowerPoint
(`.pptx`) files, and Drawings as `.png`. These files are made when they are asked for, so their
size is unknown until they arrive and they can't be resumed: a cut-off download starts again.

**Files that aren't shared with everyone.** Right-click the link in the browser and choose
**Download with DDM**, or use Drive's own **Download** button: the extension
passes your browser's Google sign-in with the download. DDM never asks for or keeps your
password. Without a sign-in, DDM says the file **needs you to sign in**.

**When Drive won't give a file.** Drive limits how often a file can be downloaded: after many
downloads in a day it answers "too many users have viewed or downloaded this file recently".
DDM shows Drive's own words and writes nothing. Try again later (it can take up to a day), or
sign in to Google in your browser and download from there, which often gets past the limit. DDM
doesn't try to get around it. A file whose owner turned off downloading, or that your account
can't open, gets **no access**.

**The video button over a Drive video** downloads the original file, not one of the player's
smaller copies.

**Folders** aren't downloaded yet: a folder's link gets **this is a Google Drive folder, not a
file**. Open the folder in the browser and download its files from there.

## Metalinks

A Metalink (`.metalink` for Metalink 3, `.meta4` for Metalink 4) is a small file that lists one
or more files, the servers that have each of them, and the hashes they must have. Release pages
of Linux distributions and other large downloads offer them next to the file itself. DDM opens
one from anywhere a download comes from: **Add URL**, the browser (add `META4` and `METALINK` to
**Options → File types** for the browser to hand them over), the command line, a drop on the main
window or the drop target, and **Tasks → Open Metalink file...**.

- **One file**: **Download File Info** opens with the file's name and its first server's address,
  as for any download.
- **Several files**: **Add Metalink** lists them, each with its size, how many servers have it
  (point at the number to see them) and what it is checked with. Untick the files you don't want,
  choose the **Category** and the folder (**Save to**; the files keep the Metalink's folders inside
  it), then **Download Now**, **Later** (to the **Main download queue**, not started) or **Cancel**.

Each file downloads from all of its servers at once: the Metalink's preferred ones first, and among
servers of the same preference those in your country (your system's region) first. A server that
fails (refuses, doesn't answer, sends another file) is left out and the others carry on. DDM uses
the other servers only when the Metalink gives the file's hash or its pieces' hashes, so a server
with a damaged copy can't spoil the file: the finished file is checked (**Checksum verified** in its
log). With pieces' hashes, a piece that fails its check is downloaded again, from another server
than the one that sent it; only if no server has it right does the download fail, keeping its data.
Logins and cookies for the Metalink's own address are never sent to its servers.

## Download File Info

DDM asks the server for the file's name and size, and shows them in **Download File Info**. The
window opens at once, even while DDM sits in the notification area: under the file's icon it says
**Checking...** while the server answers, and the size takes its place as soon as it does (a
download the browser handed over comes with its size, so there is nothing to wait for).

- **URL** — the address.
- **Category** — chosen from the file's type (Compressed, Documents, Music, Programs, Video or General). Each category has its own folder. **+** beside it makes a new category.
- **Save As** — the folder and file name. You can type another, pick one of the last 20 folders you saved to from the arrow at its end, or choose a folder with **...**.
- **Always save … files to this folder** — tick it to make the folder you typed the category's folder from now on.
- **Description** — a note of your own. It shows in the list's **Description** column and Find can search it.
- **Server** — where the server is: its country's flag and name, and how long a round trip to it takes. See [Where the server is](connections-and-speed.md#where-the-server-is). It stays blank when the browser handed the download over with its name and size, since DDM didn't need to ask the server.

![Download File Info for a ZIP file: its address, the Compressed category, the folder it goes to, its size, and the Download Later, Start Download and Cancel buttons](images/help/download-file-info.webp)
*1 the category, 2 where the file goes, 3 start now, 4 or put it in a queue.*

Then choose:

- **Start Download** (or **Enter**) — download now. The progress window opens.
- **Download Later** — add it to a queue without starting. See below.
- **Cancel** (or **Esc**) — add nothing.

By default DDM starts downloading while this window is open, so no time is lost while you decide. The download joins the list only when you choose **Start Download** or **Download Later**; **Cancel** drops what it fetched and leaves nothing behind. You can turn this off in **Options → Downloads → Start fetching while Download File Info is open**.

If the folder doesn't exist yet, DDM asks **Do you want to create it?** before it starts.

## Download Later

**Download Later** puts the download in a queue instead of starting it. By default DDM asks which one: pick the queue under **Add to queue**, and tick **Start the queue now** if you want it to run straight away. **Cancel** leaves the download in the list, in no queue.

To skip that question and always use the **Main download queue**, turn off **Options → Downloads → Ask for the queue on Download Later**.

More about queues in [Queues and the Scheduler](queues-and-scheduler.md).

## From the clipboard

With **Options → General → Offer to download links copied to the clipboard** ticked (it is, unless you turned it off), when you copy a link to a file type in **Options → File types** (ZIP, EXE, MP4, PDF and many more), **Download File Info** opens with it filled in, ready for **Start Download**. A copied magnet link opens **Add torrent** (while `TORRENT` is in the list). Links to other pages are left alone. With **Options → Downloads → Show Download File Info first** off, the download starts at once instead. Only links copied while DDM runs are taken, never one that was already there.

## Drag and drop

1. Choose **Tasks → Show drop target**. A small night-blue square with DDM's comet appears in the main window's lower-right corner and stays on top of other windows.
2. Drag a link from your browser onto it: it lights up with an amber frame while something it can take is over it. Let go, and DDM adds the link as if you had used **Add URL**.

You can also drop magnet links, `.torrent` files and Metalink files (`.metalink`, `.meta4`) on it. Drag the box to move it, and double-click it to bring the main window back. Right-click it for **Show DDM**, **Add new download** and **Hide drop target**.

## Batch download: many numbered files

For files whose names differ only by a number or a letter, such as `photo001.jpg` to `photo150.jpg`:

1. Choose **Tasks → Add batch download**. (**Tasks → Add batch download from clipboard** fills in a copied address.)
2. In **Address**, write the address with one asterisk where the number or letter changes: `https://example.com/photo*.jpg`.
3. Under **Fill the * with**, choose **Numbers** or **Letters**, then **From** and **To**.
4. For numbers, set **Digits** to the number of digits. With 3, the files are `photo001.jpg`, `photo002.jpg` and so on.
5. If the server needs a login, tick **Log in to the server**.
6. Check **First file**, **Second file** and **Last file**, then click **OK**.

The downloads go to a queue. As with Download Later, DDM asks which queue unless you turn off **Options → Downloads → Ask for the queue after a batch download**.

![Batch download: an address with an asterisk, what replaces it, and the first, second and last file](images/help/batch.webp)
*One asterisk in the address, then numbers or letters from–to.*

## A list of links

To add every link in a text file:

1. Choose **Tasks → Import → From a text file**.
2. Pick the file. **Link grabber** lists every web address DDM found in it, in packages.
3. Untick the addresses you don't want (**Select all** and **Select none** help), then click **Add to queue**.

![Link grabber listing the links of a page in packages, with their names, sizes and whether they are online](images/help/link-grabber.webp)
*Link grabber: untick what you don't want, then Add to queue.*

The downloads join the **Main download queue**, ready to start (**Start** starts it too). **Tasks → Import → From an IDM list (.ef2)** reads a list exported from IDM, with each download's referrer and cookies.

**Tasks → Link grabber...** opens the same window empty: paste text with links in it (**Ctrl+V**). See [Link grabber and archives](link-grabber.md) for the packages, the archive password and extracting archives when they are downloaded.

To save your own list, use **Tasks → Export → As a text file** (one address per line) or **As an IDM list (.ef2)**, which IDM can import.

## When the address is already in the list

By default DDM asks what to do when you add an address it already has. You can choose a fixed answer in **Options → Downloads → When an address is already in the list**:

- **Ask what to do**
- **Add it again, with a numbered file name**
- **Add it again, replacing the file**
- **Show the finished file, or resume the unfinished one**

## Next steps

- [Send browser downloads to DDM →](browser-integration.md)
- [Watch a download in the progress window →](connections-and-speed.md)
- [Queue downloads for later →](queues-and-scheduler.md)
