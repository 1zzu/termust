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
kembali. Host management dan sinkronisasi belum termasuk dan berada di fase berikutnya.

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
- Form koneksi: host atau IP, port (default 22), username, pilihan autentikasi, dan
  kredensial sesuai pilihan.
- Autentikasi password dan private key (termasuk key terenkripsi lewat passphrase).
- Verifikasi host key: fingerprint SHA-256 ditampilkan sebelum persetujuan pertama, dan
  host key tidak pernah diterima otomatis. Host yang telanjur disetujui dan kemudian
  menyajikan key berbeda akan ditolak, bukan diganti diam-diam.
- Terminal xterm.js dengan PTY jarak jauh, input keyboard, dan resize yang diteruskan ke
  server.
- Disconnect yang menutup channel, mengirim SSH disconnect, dan menghentikan task sesi.
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

## Yang sengaja belum dibuat

Persistent known-hosts, host atau credential yang tersimpan, sinkronisasi, enkripsi E2EE
lintas perangkat, akun, backend Axum, SFTP, port forwarding, jump host, snippets, web client,
Android/iOS, tim dan RBAC, plugin, serta auto-update. Local vault sudah ada, tetapi belum
menyimpan host maupun kredensial. Known-hosts pada spike ini hanya berlaku selama aplikasi
berjalan.

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

`cargo test` menjalankan test vault (lifecycle, crypto, storage, dan corruption) bersama test
SSH. Test vault memakai temporary directory dan parameter Argon2id yang ringan, jadi tidak pernah
menyentuh database atau app data pengguna.

### Live test terhadap server SSH sungguhan

Lima test terakhir di `src-tauri/src/ssh/live.rs` benar-benar membuka koneksi SSH:
password, private key, penolakan host key, host key yang berubah, dan server yang hanya
menawarkan `ssh-rsa` (SHA-1) yang harus ditolak. Semuanya diabaikan
secara default dan hanya berjalan bila diminta:

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
- **CSP.** xterm.js menyuntikkan `<style>` saat runtime, jadi `style-src` memuat
  `'unsafe-inline'`. Aman untuk aset lokal yang dimuat dari disk.
