# Termust

Termust adalah klien SSH open-source, local-first, dan cross-platform.

Repositori ini adalah **technical spike Phase 0**: bukti bahwa vertical slice berikut
bekerja dari ujung ke ujung, dengan struktur sekecil yang masih jujur.

```text
SvelteKit + shadcn-svelte
          │
       Tauri IPC
          │
      Rust + russh
          │
       SSH Server
          │
       xterm.js
```

Fokusnya membuktikan alur, bukan membangun produk. Fitur produk yang belum diperlukan
sengaja tidak ada (lihat bagian _Yang sengaja belum dibuat_).

Di atas baseline itu, Phase 1A menambahkan fondasi **encrypted local vault**: vault lokal yang
dibuat dengan master password, disimpan terenkripsi di SQLite, serta dapat dikunci dan dibuka
kembali. Phase 1B1 lalu memakai vault itu untuk menyimpan **Group, Credential, dan Host**:
CRUD lokal yang seluruhnya terenkripsi, lengkap dengan relasi Host ke Group dan Credential.
Phase 1B2 menambahkan known host yang persisten dan Connect dari saved Host, dan Phase 1C
menambahkan **snippet perintah SSH** yang tersimpan di vault yang sama.

## Prasyarat

- Node.js 22.17 atau lebih baru dan npm (batas bawah dari SvelteKit 3; Vite 8 sendiri
  meminta 20.19+ atau 22.12+).
- Rust 1.90 atau lebih baru dengan komponen `rustfmt` dan `clippy` — lihat `rust-version`
  di `src-tauri/Cargo.toml`.
- Dependensi sistem Tauri v2 untuk platform Anda: Xcode Command Line Tools (macOS),
  WebKitGTK beserta `libssl` (Linux), atau MSVC Build Tools (Windows).
- Tidak perlu `cmake` maupun NASM. `russh` di proyek ini memakai backend kripto `ring`
  alih-alih `aws-lc-rs` supaya toolchain tetap ringan di ketiga platform tersebut.

## Instalasi

```sh
npm install
```

`npm install` menjalankan `svelte-kit sync` lewat script `prepare`, yang membuat tipe dan
`tsconfig` virtual (`$app/*`) yang dipakai frontend.

## Menjalankan mode development

```sh
npm run tauri dev
```

Perintah itu membuka aplikasi desktop dan menyalakan Vite beserta hot reload. Untuk
mengembangkan frontend saja di browser, jalankan `npm run dev`; IPC Tauri tidak tersedia
di sana sehingga koneksi SSH tidak akan berjalan.

## Build

```sh
npm run tauri build
```

Build produksi memakai `adapter-static`, jadi frontend dimuat dari disk di dalam shell
Tauri. Di macOS perintah ini menghasilkan `.app`; pembuatan `.dmg` memerlukan sesi Finder
yang interaktif sehingga bisa gagal pada runner tanpa sesi grafis — itu kegagalan langkah
packaging, bukan kegagalan kode.

## Fitur yang sudah bekerja

- **Local vault (Phase 1A).** Membuat vault dengan master password, mengunci dan membukanya
  kembali, serta menyimpan object terenkripsi di SQLite. Lihat bagian _Local vault_.
- **Saved inventory (Phase 1B1).** CRUD Group, Host, dan Credential di dalam vault, termasuk
  menautkan Host ke Group dan Credential, mengganti secret credential, dan menghapus. Seluruh
  isi terenkripsi, dan secret credential tidak pernah kembali ke frontend. Lihat bagian
  _Saved inventory_.
- **Persistent known hosts (Phase 1B2a).** Host key yang disetujui disimpan sebagai object
  terenkripsi di vault, dapat dilihat dan dilupakan dari UI, dan dipakai ulang tanpa prompt.
  Lihat bagian _Known hosts_.
- **Connect dari Saved Host (Phase 1B2b).** Tombol `Connect` pada setiap saved Host membuka sesi
  SSH memakai Credential yang tertaut, tanpa mengetik ulang password atau private key. Frontend
  hanya mengirim id Host; address, username, dan secret dibaca backend dari vault. Lihat bagian
  _Saved Host connect_.
- **Encrypted snippets (Phase 1C).** Snippet perintah SSH yang disimpan terenkripsi di vault yang
  sama, dikelola dari UI, selalu ditampilkan lengkap sebelum dijalankan, dan hanya dikirim ke session
  SSH yang sudah aktif setelah pengguna menekan tombol konfirmasi. Lihat bagian _Snippets_.
- **Import Host dari basic OpenSSH config (Phase 1D).** Subset kecil yang jujur: `Host`, `HostName`,
  `User`, dan `Port` dari file config atau teks yang ditempel, selalu lewat preview dan konfirmasi,
  tanpa membaca key atau file lain. Lihat bagian _Import OpenSSH config_.
- **Encrypted backup & restore (Phase 1E).** Seluruh domain data vault ditulis ke satu file
  `.termust-backup` yang terenkripsi dengan password khusus backup, dapat diperiksa tanpa menulis apa
  pun, dan dapat dipulihkan secara atomik ke vault mana pun — termasuk vault dengan master password
  berbeda, karena setiap record dienkripsi ulang dengan Vault Key vault tujuan. Lihat bagian
  _Backup & Restore_.
- **Multi-session terminal tabs (Phase 1F).** Beberapa sesi SSH berjalan bersamaan dalam satu window,
  satu tab untuk setiap sesi, masing-masing dengan terminal, status, dan scrollback sendiri. Lihat
  bagian _Multi-session terminal tabs_.
- **Quick Connect.** Form koneksi langsung tanpa menyimpan host: host atau IP, port (default 22),
  username, pilihan autentikasi, dan kredensial sesuai pilihan. Form ini tetap terpisah dan tidak
  memakai saved Host maupun saved Credential. Setiap Connect membuka tab baru, sehingga beberapa
  koneksi dapat berjalan sekaligus.
- Autentikasi password dan private key (termasuk key terenkripsi lewat passphrase).
- Verifikasi host key: fingerprint SHA-256 ditampilkan sebelum persetujuan pertama, dan
  host key tidak pernah diterima otomatis. Persetujuan disimpan di vault, jadi koneksi
  berikutnya ke host yang sama tidak bertanya lagi — termasuk setelah aplikasi di-restart.
  Host yang telanjur disetujui dan kemudian menyajikan key berbeda akan ditolak, bukan
  diganti diam-diam.
- Terminal xterm.js dengan PTY jarak jauh, input keyboard, dan resize yang diteruskan ke
  server. Setiap tab memakai instance `Terminal`, `FitAddon`, dan container-nya sendiri.
- Disconnect yang menutup channel, mengirim SSH disconnect, dan menghentikan task sesi tanpa
  menghapus tabnya; tombol close pada tab menghentikan sesi tab itu saja.
- Pesan error yang bisa ditindaklanjuti tanpa mengulang isi kredensial.

## Local vault (Phase 1A)

Vault adalah satu database SQLite yang dienkripsi pada level object. Rantai kuncinya pendek:

```text
master password --Argon2id--> Key Encryption Key
Key Encryption Key --XChaCha20-Poly1305--> Vault Key (32 byte acak)
Vault Key --XChaCha20-Poly1305--> payload object
```

- Database berada di application data directory milik sistem operasi, bukan di dalam repositori:
  `~/Library/Application Support/com.termust.desktop/termust.db` (macOS),
  `~/.local/share/com.termust.desktop/termust.db` (Linux), atau
  `%APPDATA%\com.termust.desktop\termust.db` (Windows).
- Master password tidak pernah disimpan, dan tidak ada hash master password yang dipakai untuk
  membuka vault. Yang tersimpan hanya wrapped Vault Key, salt, dan parameter Argon2id.
- Karena itu master password **tidak dapat dipulihkan**. Hilangnya master password berarti
  hilangnya akses ke isi vault.
- Selama vault terbuka, Vault Key hanya ada di memori proses dan di-zeroize saat vault dikunci
  atau saat aplikasi ditutup.
- Phase 1A belum memiliki biometric unlock, OS keychain, Tauri Stronghold, auto-lock timer, host
  atau credential persistence, maupun sinkronisasi. UI hanya menyediakan create, unlock, lock,
  dan status.

Format penyimpanan dan versioning dijelaskan di
[`docs/vault-format-v1.md`](docs/vault-format-v1.md).

## Saved inventory (Phase 1B1)

Group, Host, dan Credential disimpan sebagai object di vault yang sama: satu row `vault_objects`
per record, dengan `kind` `group`, `host`, atau `credential`. Tidak ada tabel baru dan tidak ada
perubahan pada crypto Phase 1A.

- CRUD lengkap untuk ketiganya, dan semuanya menuntut vault dalam keadaan unlocked. Saat vault
  terkunci seluruh command ditolak dan state inventory di frontend dibersihkan.
- Host dapat menunjuk satu Group dan satu Credential; relasinya divalidasi setelah payload
  didekripsi. Group atau Credential yang masih dipakai Host tidak dapat dihapus, dan tidak ada
  cascade delete.
- Delete memakai tombstone, sama seperti object vault lainnya.
- Credential secret — password, private key, dan passphrase — hanya berjalan satu arah: dikirim ke
  Rust saat create atau replace, dan tidak pernah dikembalikan. Daftar dan detail hanya
  mengembalikan summary tanpa field secret, dan form edit tidak memuat secret lama.

Model data lengkapnya ada di [`docs/local-data-model-v1.md`](docs/local-data-model-v1.md).

## Saved Host connect (Phase 1B2b)

Setiap saved Host yang punya Credential menampilkan tombol `Connect`. Host tanpa Credential
menampilkan `Credential required` dan tombolnya tidak aktif; Credential dipilih lewat `Edit` pada
Host, bukan lewat dialog saat connect.

- Frontend hanya mengirim `sessionId`, `hostId`, `cols`, dan `rows`. Address, port, username, jenis
  autentikasi, dan secret seluruhnya diselesaikan backend dari vault — payload itu menolak field
  lain (`deny_unknown_fields`), jadi frontend tidak dapat mengarahkan koneksi ke server atau
  Credential lain, dan tidak ada secret yang melewati IPC.
- Secret credential tidak pernah dikembalikan ke frontend, tidak masuk event, tidak masuk error,
  dan tidak masuk form Quick Connect. Yang dikirim hanya request internal yang dibaca
  authentication, dan byte secret-nya di-zeroize saat sesi selesai.
- Vault harus unlocked untuk memulai sesi dari saved Host. Kalau vault terkunci, command ditolak
  dengan pesan yang jelas dan tidak ada koneksi jaringan yang dibuka. Quick Connect tetap dapat
  dipakai tanpa vault.
- Mengunci vault setelah sesi berjalan **tidak** memutus sesi itu: koneksi sudah selesai
  terautentikasi dan tidak menyimpan apa pun untuk reconnect. Sesi baru dari saved Host tetap
  memerlukan unlock ulang, dan tidak ada reconnect otomatis.
- Sesi memakai lifecycle yang sama dengan Quick Connect, termasuk verifikasi host key Phase 1B2a
  (approval pertama, trust lintas restart, dan blokir saat key berubah).
- Setiap klik `Connect` membuka tab baru, termasuk koneksi duplikat ke saved Host yang sama. Klik
  ganda pada host yang sama diabaikan selama tab host itu masih connecting; setelah request-nya
  diterima backend, koneksi berikutnya boleh dibuka. Kegagalan satu koneksi tidak memengaruhi tab
  lain, dan label saved host menjadi title tabnya.

## Known hosts (Phase 1B2a)

Host key yang disetujui disimpan sebagai object `known_host` di vault yang sama, satu record per
identitas target (host dan port). Tidak ada tabel baru dan tidak ada perubahan pada crypto
Phase 1A.

- Koneksi pertama ke sebuah host menampilkan dialog fingerprint; menyetujui berarti key disimpan
  ke vault, dan koneksi berikutnya ke host itu berjalan tanpa dialog.
- Perbandingan memakai algoritma **dan** fingerprint.
- Host key yang berubah memblokir koneksi, menampilkan fingerprint lama dan baru, dan tidak pernah
  diganti otomatis. Tidak ada tombol “accept changed key”: pengguna harus melupakan key lama lebih
  dulu.
- Saat vault terkunci, record tidak dapat dibaca, jadi tiap host dianggap belum dikenal dan
  approval hanya berlaku selama aplikasi berjalan. Dialognya menjelaskan hal itu, dan tidak ada
  vault yang dibuka otomatis. Ini juga yang membuat Quick Connect tetap bekerja tanpa vault.
- Known host dapat dilihat dan dilupakan dari UI; melupakan berarti koneksi berikutnya akan
  bertanya lagi. Menyimpan hanya terjadi melalui prompt SSH: tidak ada command yang menulis
  fingerprint dari frontend.
- Nama host dan fingerprint berada di dalam payload terenkripsi, bukan di kolom plaintext.

Aturan identitas dan payload-nya ada di [`docs/local-data-model-v1.md`](docs/local-data-model-v1.md).

## Snippets (Phase 1C)

Snippet adalah perintah SSH pendek yang disimpan sebagai object terenkripsi di vault yang sama,
dengan `kind` `snippet`. Tidak ada tabel baru dan tidak ada perubahan pada crypto Phase 1A. Snippet
tidak punya relasi ke Host, Group, atau Credential.

- CRUD lengkap (`list_snippets`, `create_snippet`, `update_snippet`, `delete_snippet`), semuanya
  menuntut vault dalam keadaan unlocked. Saat vault terkunci seluruh command ditolak dan daftar
  snippet di frontend dibersihkan, termasuk dialog yang sedang terbuka.
- Delete memakai tombstone, sama seperti object vault lainnya.
- Nama dan isi snippet berada di dalam payload terenkripsi, jadi tidak ada kolom plaintext database
  yang memuatnya. Panjang nama 1–120 karakter setelah trim; isi 1 byte sampai 64 KiB UTF-8, tanpa
  NUL byte. Nama tidak harus unik: `id` adalah identitas sebenarnya.
- Daftar diurutkan berdasarkan nama (tanpa memandang huruf besar/kecil) lalu `id`, sehingga urutannya
  stabil.
- Menjalankan snippet (`run_snippet`) hanya mengirim `sessionId` dan `snippetId`. Backend membaca isi
  dari vault, memastikan session SSH masih aktif, lalu mengirim isi itu ke PTY jarak jauh memakai
  primitive input yang sama dengan keyboard terminal — bukan shell lokal, bukan `Command`, dan tidak
  ada jalur input kedua. Satu karakter Enter dikirim terpisah, dan tidak ditambahkan bila isi sudah
  berakhir dengan baris baru.
- Snippet selalu melewati preview dialog: nama, isi lengkap dengan font monospace, target session,
  dan peringatan bahwa command akan dikirim ke shell yang sedang aktif. Menutup dialog sama dengan
  cancel, dan hanya tombol konfirmasi yang memanggil `run_snippet`. Isi ditampilkan sebagai teks,
  bukan HTML.
- Tidak ada auto-run: snippet tidak dijalankan saat dibuat, diedit, didaftar, saat vault dibuka,
  maupun saat koneksi dibuat. Tidak ada riwayat eksekusi dan tidak ada shortcut keyboard.
- Termust tidak memahami sintaks shell: isi snippet tidak diparse, tidak dinormalisasi, dan tidak
  diperiksa. Snippet juga **bukan** tempat menyimpan password, token, atau private key — itu tetap
  milik Credential, dan secret variables adalah fitur terpisah di masa depan.

Batas ukuran, validasi, dan bentuk payload-nya ada di
[`docs/local-data-model-v1.md`](docs/local-data-model-v1.md).

## Import OpenSSH config (Phase 1D)

Import Host dari subset dasar OpenSSH client config. Ini **bukan** parser OpenSSH yang kompatibel
penuh: yang dibaca adalah `Host`, `HostName`, `User`, dan `Port`, satu alias literal per block,
dan seluruh directive lain dilaporkan sebagai catatan pada barisnya, bukan ditafsirkan diam-diam.

- Parser berada di Rust, murni terhadap input-nya: tidak membuka file, tidak menjalankan proses,
  tidak membaca environment, tidak melakukan DNS lookup, dan tidak membuka koneksi jaringan.
- `preview_openssh_config` menerima `{ content }` dan tidak menulis apa pun. `import_openssh_hosts`
  menerima `{ content, selectedEntryIndexes }`, memparse ulang content itu dengan parser yang sama,
  dan hanya membuat Host untuk entry berstatus `ready` yang dipilih. Field lain ditolak
  (`deny_unknown_fields`), termasuk path — jadi tidak ada file yang dapat ditunjuk dari config atau
  dari frontend.
- `IdentityFile`, `CertificateFile`, `IdentityAgent`, dan `Include` tidak pernah dibaca dan tidak
  pernah diikuti. Import tidak membuat Credential, tidak membuat Group, tidak menimpa Host yang ada,
  dan tidak menghapus apa pun: Host hasil import punya `credentialId = null` sampai pengguna
  mengeditnya.
- Duplicate detection memakai identity ternormalisasi `address + port + username`, jadi mengimpor
  file yang sama dua kali tidak membuat Host kedua. Tombstone tidak dianggap Host aktif, dan label
  yang sama dengan target berbeda menambah Host baru, bukan menimpa yang lama.
- Import **tidak atomic**: Host ditulis satu per satu lewat service Host yang sama, dan hasil per
  entry (`imported`, `skipped`, `failed`) dilaporkan apa adanya. Mengulang import yang sama aman.
- Config maksimum 1 MiB, divalidasi di frontend dan diulang di backend. Content tidak pernah masuk
  `localStorage` atau log, dan dibuang dari state saat dialog ditutup, import selesai, atau vault
  dikunci.

Subset sintaks, aturan duplicate, batas keamanan, dan langkah pemakaiannya ada di
[`docs/openssh-import-v1.md`](docs/openssh-import-v1.md). Model Host tidak berubah sama sekali:
lihat [`docs/local-data-model-v1.md`](docs/local-data-model-v1.md).

## Backup & Restore (Phase 1E)

Seluruh isi vault dapat ditulis ke satu file `.termust-backup` yang terenkripsi dengan **password
khusus backup**, lalu dipulihkan lagi. File itu portable: backup yang dibuat dari vault dengan master
password A dapat dipulihkan ke vault dengan master password C, karena setiap record dienkripsi ulang
memakai Vault Key vault tujuan.

- **Export.** Dialog meminta password backup dua kali (minimum 12 karakter, tidak boleh sama dengan
  master password vault secara diam-diam — pengguna boleh memilih yang sama, aplikasi tidak tahu),
  lalu membuka native save dialog. File ditulis secara atomik: temporary file di direktori tujuan yang
  sama, `sync`, lalu dipublikasikan dengan replace move (Unix `rename`, Windows
  `MoveFileExW`/`MOVEFILE_REPLACE_EXISTING`), dengan mode `0600` di Unix. Tanpa persetujuan overwrite,
  publikasinya adalah no-clobber move yang diputuskan filesystem — bukan check-then-rename yang bisa
  kalah race. File temporary dibersihkan lewat RAII bila gagal. Backup hanya membaca vault, jadi export
  boleh dilakukan saat session SSH aktif, dan tidak ada object atau timestamp yang berubah.
- **Inspect.** `inspect_encrypted_backup` membuka file **tanpa menulis apa pun** dan hanya
  mengembalikan ringkasan: `createdAt`, `sourceAppVersion`, jumlah object, jumlah aktif, jumlah
  tombstone, `countByKind`, dan `fileDigest` (SHA-256 file itu, bukan rahasia). Tidak ada secret,
  nama, address, atau payload utuh yang sampai ke frontend: preview menunjukkan berapa banyak
  datanya, bukan isinya. Digest itu dipakai restore untuk memastikan file yang dipulihkan memang file
  yang baru saja dibaca.
- **Restore.** Hanya mode `replace`. Frontend menjelaskan bahwa seluruh isi vault akan diganti,
  meminta pengguna mengetik `RESTORE`, dan mengirim `mode` + `confirmation` + `fileDigest` ke backend
  — yang memeriksanya sendiri, bukan hanya mengandalkan tombol yang disabled. Restore ditolak bila
  ada session SSH aktif atau connecting, dan aplikasi tidak pernah memutus session itu sendiri.
  Pemeriksaan session itu dan penguncian terhadap session baru terjadi dalam satu langkah atomik:
  selama restore berjalan, `connect_ssh`/`start_saved_ssh` ditolak. Guard itu dimiliki oleh pekerjaan
  blocking restore — bukan oleh future command — sehingga command yang dibatalkan tidak membuka jalan
  untuk session baru selama restore masih menulis ke vault, dan guard-nya dilepas lewat RAII pada
  sukses, gagal, maupun panic.
- **Atomik.** Validasi seluruh snapshot terjadi sebelum satu baris pun diubah, lalu satu transaksi
  SQLite menghapus seluruh row lama dan menulis seluruh snapshot. Restore yang gagal tidak mengubah
  vault sama sekali, dan `vault_meta` tidak pernah disentuh: master password, Vault Key, dan parameter
  Argon2 vault tujuan tetap dipakai setelah restore.
- **Portable.** Backup tidak memuat ciphertext vault sumber, wrapped Vault Key, atau metadata vault —
  hanya record domain yang dienkripsi ulang saat restore. Karena itu master password vault sumber dan
  password backup tidak dapat membuka vault tujuan.
- **Tombstone.** Object yang sudah dihapus ikut tersimpan sebagai tombstone dan tetap terhapus setelah
  restore; restore mengganti seluruh isi vault, jadi record yang dibuat setelah backup juga hilang.
- **Fail closed.** Object dengan kind yang tidak dikenal, payload version yang tidak didukung, atau
  crypto version yang lebih baru membuat export gagal seluruhnya, bukan menghasilkan backup yang
  kehilangan data. Parameter KDF dari file dibatasi sebelum Argon2id dijalankan, dan password salah
  atau file rusak menghasilkan satu pesan generik yang sama.

Detail format, envelope, AAD, validasi snapshot, dan penanganan file ada di
[`docs/backup-format-v1.md`](docs/backup-format-v1.md). Backup password tidak dapat dipulihkan: file
itu tidak akan bisa dibuka lagi kalau passwordnya hilang, dan backup bukan sync — restore mengganti,
bukan menggabungkan.

## Multi-session terminal tabs (Phase 1F)

Beberapa sesi SSH dapat berjalan bersamaan dalam satu window, satu tab untuk setiap sesi. Backend
sudah menyimpan sesinya di dalam map berdasarkan `sessionId` sebelum fase ini, jadi yang ditambahkan
adalah state sesi yang eksplisit di frontend — bukan arsitektur backend yang baru.

- **State sesi.** Satu daftar `TerminalSession` (`id`, `title`, `source`, `status`, `statusMessage`,
  `errorMessage`, `hostKeyPrompt`, `disconnecting`, `closing`) plus satu `activeSessionId`. ID tab dibuat dengan
  `crypto.randomUUID()`, urutan tab stabil, dan `activeSessionId` selalu `null` atau menunjuk tab
  yang masih ada. Tidak ada descriptor yang menyimpan password, private key, passphrase, credential
  id, atau secret lain — termasuk untuk tab dari saved Host. Fungsi state-nya murni dan berada di
  `src/lib/session-tabs.ts`.
- **Terminal per tab.** Setiap tab punya `Terminal`, `FitAddon`, container DOM, scrollback, input
  handler, dan `ResizeObserver` sendiri, dimiliki oleh komponen tabnya
  (`src/lib/components/terminal-session.svelte`). Berpindah tab hanya menyembunyikan terminal yang
  tidak aktif, tidak pernah membuangnya, sehingga scrollback tetap ada dan output tetap masuk ke
  terminal yang benar meskipun tabnya sedang tidak terlihat. `fit()` tidak pernah dijalankan pada
  container tersembunyi yang berukuran nol, dan resize satu tab tidak pernah mengubah tab lain.
  Koneksi baru dimulai setelah terminal tab itu siap **dan** keempat listener event sudah terpasang,
  jadi event awal tidak hilang: bila salah satu listener gagal dipasang, semua listener dilepas
  kembali dan tidak ada sesi yang dimulai sama sekali. Start sebuah tab bisa dibatalkan: tab yang
  ditutup sebelum IPC tidak pernah memanggil backend, dan tab yang ditutup saat IPC berjalan
  ditunggu sampai request-nya selesai sebelum diputus dan dihapus. Lifecycle ini ada di
  `src/lib/session-start.ts` dan `src/lib/session-lifecycle.ts`.
- **Routing memakai `sessionId` saja.** Keempat event yang sudah ada (`ssh://data`, `ssh://status`,
  `ssh://error`, `ssh://host-key`) dipetakan hanya lewat `sessionId` yang dibawanya: bukan lewat
  title, hostname, urutan tab, atau tab yang sedang aktif saat event datang. Output sesi A tidak
  pernah masuk ke terminal B, status/error/prompt disimpan pada tab asalnya, dan event untuk tab yang
  sudah ditutup diabaikan.
- **Tab strip** di atas terminal memakai komponen shadcn-svelte yang sudah ada, dengan
  `role="tablist"`, `role="tab"`, `role="tabpanel"`, tabindex roving, dan keyboard Arrow Left,
  Arrow Right, Home, serta End. Setiap tab menampilkan status dot (connecting/checking host
  key/authenticating, connected, disconnected, error), title, dan tombol close berlabel aksesibel;
  stripnya bisa di-scroll horizontal dan tidak menambah tinggi window. Tidak ada custom title bar di
  fase ini.
- **Host-key prompt** disimpan pada tab asalnya. Prompt yang datang untuk tab yang tidak aktif
  membawa tab itu ke depan (kecuali tab di layar sedang menampilkan prompt lain), dan jawabannya
  selalu memakai `sessionId` milik prompt itu, bukan tab yang sedang aktif. Prompt yang datang
  berdekatan semuanya tersimpan sampai dijawab.
- **Disconnect dan close adalah dua tindakan berbeda.** Disconnect memanggil `disconnect_ssh` hanya
  untuk tab aktif dan menandai tabnya `disconnecting` lebih dulu: input/resize tab itu berhenti,
  tombol Disconnect mati, dan tab dihitung masih hidup sampai backend benar-benar selesai — jadi
  restore tetap tidak mungkin dimulai di tengah proses. Setelah selesai, tab tetap ada beserta
  scrollback-nya dan statusnya menjadi `disconnected`; bila IPC gagal, tandanya diambil kembali, tab
  tetap hidup, dan error-nya ditampilkan agar bisa dicoba lagi. Close membatalkan start tab itu bila
  masih berjalan, menunggu start tersebut selesai, baru menghentikan sesinya — dan hanya menghapus
  tabnya setelah stop benar-benar berhasil. Tombol close pada tab lain tidak terpengaruh dan klik
  close berulang diabaikan. Tab berikutnya yang aktif dipilih dari kanan pada posisi yang sama, lalu
  kiri, dan `activeSessionId` menjadi `null` bila tidak ada tab tersisa. Saat window dibongkar,
  seluruh start yang masih pending dibatalkan lebih dulu (tidak ada sesi baru yang lahir setelah
  teardown dimulai), lalu seluruh sesi yang masih hidup diputus dengan best effort.
- **Snippet** hanya berjalan pada tab aktif, dan dialog preview-nya mengikat `sessionId` serta title
  saat dibuka: berpindah tab menonaktifkan tombol konfirmasi alih-alih diam-diam mengirim command ke
  tab lain.
- **Backup/restore** memakai `hasLiveSessions`, yang dihitung dari seluruh tab (connecting,
  verifying_host_key, authenticating, connected, atau sedang closing), bukan hanya tab aktif. Restore
  tetap ditolak backend selama satu sesi pun masih hidup; export tetap boleh kapan saja. ID sesi yang
  masih terpakai ditolak backend, bukan diganti, sehingga satu ID tidak pernah menunjuk dua sesi.
- **Yang belum ada di fase ini:** split pane, tab drag-and-drop, tab pinning, tab groups, broadcast
  input, duplicate session, SSH multiplexing, reconnect otomatis, dan session persistence. Tab tidak
  dipulihkan setelah aplikasi di-restart, dan restore terminal scrollback belum ada.

## Yang sengaja belum dibuat

split pane, tab drag-and-drop, tab pinning, tab groups, broadcast input, reconnect otomatis, session restore, auto-run
command, SSH agent, certificate, keyboard-interactive, credential prompt fallback, parser OpenSSH
lengkap (`Match`, `Include`, wildcard, `Host *` default, multiple alias per block), import
`IdentityFile` atau private key dari config, import `known_hosts` OpenSSH, wildcard host, jump host,
menulis atau meng-export config OpenSSH, SFTP, port forwarding, tags, notes,
search, snippet variables, snippet tags, snippet folders, snippet history, snippet sharing atau sync,
cloud backup, backup otomatis atau terjadwal, incremental backup, merge restore, conflict resolution,
per-object restore, unencrypted export, dan export ke OpenSSH config,
auto-lock timer, keychain/biometric unlock, recovery key, sinkronisasi,
enkripsi E2EE lintas perangkat, akun, backend Axum/self-hosted, web SSH relay, mobile UI, tim dan
RBAC, plugin, serta auto-update. Beberapa sesi dapat berjalan bersamaan, tetapi tab tidak dipulihkan
setelah aplikasi ditutup, dan reconnect otomatis belum ada.

## Pengujian

Perintah npm dijalankan dari akar repositori:

```sh
npm run lint      # prettier --check . && eslint .
npm run check     # svelte-check
npm test          # vitest
npm run build
```

Perintah cargo dijalankan dari `src-tauri/`:

```sh
cd src-tauri
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

`cargo test` menjalankan test vault (lifecycle, crypto, storage, dan corruption), test backup
(format, envelope, penolakan file rusak, snapshot validation, atomic restore, rollback penuh, dan
restore ke master password berbeda), test inventory
(Group, Host, Credential, known host, relasi, resolusi Host + Credential untuk koneksi, dan handling
secret), test snippet (CRUD, validasi batas, tombstone, isi yang tidak diubah, dan pengiriman ke
session), test trust host key, serta test SSH. Semuanya memakai temporary directory dan parameter
Argon2id yang ringan, jadi tidak pernah menyentuh database atau app data pengguna.

Test yang menyentuh Tauri command (`connect_ssh`, `start_saved_ssh`, `list_known_hosts`,
`forget_known_host`, `list_snippets`, `create_snippet`, `update_snippet`, `delete_snippet`,
`run_snippet`, `preview_openssh_config`, `import_openssh_hosts`, `export_encrypted_backup`,
`inspect_encrypted_backup`, `restore_encrypted_backup`) memakai mock runtime milik Tauri,
sehingga nama command, nama argument, dan bentuk payload yang diterima frontend diuji lewat dispatch
IPC yang sebenarnya — bukan lewat salinannya.
Termasuk bahwa `start_saved_ssh` menolak payload yang mencoba membawa password, private key,
passphrase, address, atau credential id, dan bahwa `run_snippet` hanya menerima `sessionId` dan
`snippetId`: payload yang mencoba membawa `content`, `command`, atau field lain ditolak, bukan
diabaikan.

State tab multi-sesi diuji sebagai fungsi murni di `src/lib/session-tabs.test.ts`: dua tab
menghasilkan dua session ID, perpindahan tab tidak menghapus tab lain, event dirutekan berdasarkan
session ID, output satu sesi tidak masuk ke terminal lain, status/error/host-key prompt satu tab
tidak mengubah tab lain, pemilihan tab setelah close (kanan lalu kiri, dan `null`), disconnect yang
mempertahankan tab, tab disconnected yang tidak lagi mengirim input atau resize, `hasLiveSessions`
yang melihat seluruh tab, snippet yang terikat pada session ID saat preview dibuka, descriptor tab
yang tidak memuat credential, request saved Host yang tetap hanya membawa empat field, cleanup yang
memutus seluruh sesi hidup, dan event terlambat untuk tab yang sudah ditutup. `session-start.test.ts`
menguji lifecycle satu start (queued/invoking/finished, cancellation sebelum IPC, dan pelepasan plan
beserta kredensialnya: plan dipegang satu binding milik start, objek dependency-nya tidak pernah
membawa plan, callback penutupnya hanya menerima `sessionId`, dan plan tetap tidak terbaca lagi
setelah start selesai — sukses, gagal, atau dibatalkan), sedangkan `session-lifecycle.test.ts`
menguji urutannya: tidak ada start
sebelum listener lengkap, listener yang gagal melepas listener parsial dan tidak memulai backend,
close sebelum listener siap tidak memanggil backend, close saat IPC berjalan menunggu start lalu
disconnect lalu menghapus tab, close yang gagal menghentikan sesi mempertahankan tabnya, disconnect
yang sedang berjalan tetap dihitung hidup dan tidak menerima input, serta teardown yang membatalkan
seluruh pending start. Semuanya deterministik, memakai channel dan dependency injection, tanpa timer
sebagai sinkronisasi. Di sisi Rust, `ssh::tests` membuktikan dua sesi dapat diregistrasi bersamaan,
input/resize/jawaban host key hanya sampai ke sesi yang dituju, disconnect satu sesi tidak mengganggu
sesi lain, ID sesi yang masih terpakai ditolak alih-alih diganti, dan restore guard baru dapat
diambil setelah seluruh sesi selesai. Untuk lifecycle stop, `ssh::tests` juga membuktikan sesi yang
sedang stopping tetap menolak restore guard dan tidak bisa dipakai ulang ID-nya sampai task-nya
benar-benar selesai, penanda stopping dilepas pada jalur graceful maupun abort, task yang selesai
sangat cepat tidak meninggalkan sesi basi, dan task yang selesai terlambat tidak bisa menghapus sesi
yang lebih baru.

### Live test terhadap server SSH sungguhan

Test di `src-tauri/src/ssh/live.rs` benar-benar membuka koneksi SSH: password, private key,
penolakan host key, host key yang berubah, host key yang diingat setelah vault dibuka ulang, dan
server yang hanya menawarkan `ssh-rsa` (SHA-1) yang harus ditolak. Untuk Phase 1B2b, test yang sama
membuka sesi lewat jalur saved Host (Credential password dan private key dari vault), mengulanginya
tanpa prompt host key setelah vault dibuka ulang, serta membuktikan password salah dan host key yang
berubah gagal tanpa mengulang secret di pesan error. Untuk Phase 1C, `live_snippet_runs_on_the_remote_shell`
menyimpan snippet ke vault sementara, membuka session sungguhan, menyalurkan snippet lewat jalur input
yang sama dengan aplikasi, dan membuktikan shell jarak jauh benar-benar menjalankannya — termasuk
snippet dua baris — lalu session itu masih menerima input keyboard dan diputus dengan bersih. Untuk
Phase 1D, `live_imported_host_connects_after_a_credential_is_linked` mengimpor config yang menunjuk
server itu, membuktikan Host hasil import belum punya Credential, membuat Credential lewat service
normal, menautkannya ke Host tersebut, lalu membuka shell dan menjalankan command marker lewat jalur
saved Host yang sama seperti Host biasa. Untuk Phase 1E,
`live_backup_round_trip_restores_a_host_a_known_host_and_a_snippet` membuat vault sumber (Group,
Credential, Host, Known host hasil approval, Snippet), mengekspor backup sambil session pertama masih
terbuka, membuktikan restore **ditolak** selama session itu hidup, memutusnya, memulihkan backup ke
vault kedua dengan master password berbeda, memastikan vault itu tetap hanya terbuka dengan password
miliknya, lalu membuka session lewat Host hasil restore tanpa prompt host key dan menjalankan Snippet
hasil restore di shell sungguhan. Untuk Phase 1F, `live_two_sessions_stay_independent` membuka dua
shell SSH nyata secara bersamaan, mengirim marker berbeda ke masing-masing, membuktikan output tidak
tertukar, memutus sesi pertama, lalu memastikan sesi kedua masih menjalankan command sebelum ditutup
dengan bersih.
Semuanya diabaikan secara default dan hanya berjalan bila diminta:

```sh
cd src-tauri
TERMUST_LIVE_HOST=host TERMUST_LIVE_PORT=22 TERMUST_LIVE_USER=user TERMUST_LIVE_PASSWORD=secret \
    cargo test live_ -- --ignored --nocapture

TERMUST_LIVE_HOST=host TERMUST_LIVE_USER=user \
    TERMUST_LIVE_PRIVATE_KEY_PATH=~/.ssh/id_ed25519 \
    cargo test live_private_key -- --ignored --nocapture
```

Private key dibaca dari `TERMUST_LIVE_PRIVATE_KEY_PATH` (path, hanya dibaca) atau
`TERMUST_LIVE_PRIVATE_KEY` (isinya), dan passphrase dari `TERMUST_LIVE_PASSPHRASE`.
Semua kredensial hanya berasal dari environment, tidak ada yang ditulis ke file, dan tidak
ada yang dicetak. Bila environment variable-nya tidak ada, test melaporkan alasannya dan
tidak melakukan apa-apa.

### Integration test dengan server sekali pakai

Test live yang sama juga bisa dijalankan tanpa menyiapkan server sendiri:

```sh
npm run test:ssh-integration
```

Script `scripts/test-ssh-integration.sh` membangun image kecil dari
`tests/integration/ssh`, menjalankan dua container OpenSSH pada port dinamis di
`127.0.0.1` (satu biasa, satu yang hanya menerima tanda tangan legacy `ssh-rsa`),
membuat password dan key sementara di temporary directory, lalu menghapus container,
key, dan password itu lagi lewat `trap` — termasuk ketika test gagal atau dihentikan.

Bedanya dengan unit test: `npm test` dan `cargo test` tidak pernah membuka jaringan dan
hanya butuh toolchain. Integration test benar-benar membuka koneksi SSH ke server nyata,
butuh Docker yang sedang berjalan (plus `ssh-keygen` dan `openssl` dari OpenSSH client),
dan tidak pernah berjalan tanpa command eksplisit di atas. Tidak ada server milik siapa
pun yang dihubungi: seluruh kredensial dibuat saat itu juga dan tidak disimpan.

### Checklist manual

Hal yang tidak bisa dibuktikan otomatis — dialog host key, keyboard dan paste, resize,
serta isi log — ada di [`docs/phase-0-manual-checklist.md`](docs/phase-0-manual-checklist.md).
Semua item di sana masih berstatus belum diuji.

## CI

`.github/workflows/ci.yml` menjalankan lint, check, unit test, dan build frontend serta
`cargo fmt`, `cargo clippy`, dan `cargo test` di Ubuntu, macOS, dan Windows. Di setiap
platform aplikasi Tauri juga dikompilasi dengan `tauri build --no-bundle`, sehingga build
yang gagal dikompilasi terlihat lebih awal. Integration test SSH berjalan di job terpisah
pada Ubuntu karena memerlukan Docker.

CI sengaja tidak membuat installer: tanpa `.dmg`, `.msi`, atau `deb`, signing, Finder, dan
lingkungan packaging tidak bisa menggagalkan build. Tidak ada secret repositori yang
dipakai dan tidak ada server eksternal yang dihubungi.

## Kebijakan algoritma SSH

Klien ini tidak menawarkan maupun menerima algoritma legacy:

- Daftar algoritma dimulai dari `russh::Preferred::DEFAULT` lalu `ssh-rsa` — satu-satunya
  entri yang menandatangani dengan SHA-1 — dibuang. Sisanya dibiarkan apa adanya, termasuk
  algoritma yang ditambahkan versi `russh` berikutnya.
- Untuk key RSA, tanda tangan wajib memakai hash SHA-2. Server yang hanya bisa
  `ssh-rsa` (SHA-1), atau yang tidak mengumumkan dukungan RSA sama sekali, ditolak dengan
  pesan yang menyarankan key Ed25519 atau ECDSA — tidak ada fallback diam-diam.

Aturan ini dikunci oleh unit test (`ssh::session::tests`, `ssh::auth::tests`) dan diuji
nyata terhadap server yang dikonfigurasi hanya menerima `ssh-rsa`.

## Catatan implementasi

- **Dependency vault.** Vault memakai `rusqlite` (SQLite), `argon2` (Argon2id),
  `chacha20poly1305` (XChaCha20-Poly1305), `zeroize` (menghapus key dari memori), `uuid`, dan
  `getrandom`. Tidak ada framework crypto tambahan.
- **SQLite `bundled`.** `rusqlite` dikompilasi dengan fitur `bundled`, sehingga SQLite dibangun
  dari sumber dan tidak bergantung pada library SQLite sistem. Konsekuensinya sebuah C compiler
  harus tersedia, sama seperti yang sudah dibutuhkan `ring`.
- **Import `#lib`.** Kode memakai subpath import dari `package.json` (`imports`), bukan opsi
  `alias` SvelteKit yang sudah deprecated. `tsconfig.json` mendeklarasikan pemetaan yang sama
  karena TypeScript tidak me-resolve target `imports` tanpa ekstensi, sedangkan Vite bisa.
- **shadcn-svelte.** Alias di `components.json` memakai `#lib`, jadi komponen baru ditulis
  dengan import yang sama. CLI-nya membaca `tsconfig` yang meng-`extends` `$app/tsconfig`,
  sehingga jalankan `npm run prepare` lebih dulu bila direktori virtual itu belum ada.
- **Backend `ring`.** `russh` dikonfigurasi dengan fitur `ring` supaya build tidak butuh
  `cmake`/NASM.
- **Native dialog backup.** Phase 1E memakai plugin resmi `tauri-plugin-dialog` (Rust) dan
  `@tauri-apps/plugin-dialog` (frontend) hanya untuk satu hal: memilih lokasi file backup. Plugin itu
  mengembalikan path ke frontend dan tidak pernah membaca atau menulis file sendiri, jadi tidak ada
  plugin filesystem yang dipasang dan scope-nya minimum: `dialog:allow-open` dan `dialog:allow-save`
  saja (lihat `src-tauri/capabilities/default.json`). Penulisan file, validasi path, dan batas
  ukurannya tetap di backend.
- **CSP.** xterm.js menyuntikkan `<style>` saat runtime, jadi `style-src` memuat
  `'unsafe-inline'`. Aman untuk aset lokal yang dimuat dari disk.
