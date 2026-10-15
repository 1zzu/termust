# Termust

<p align="center">
  <strong>Fast, Local-First, and Secure Cross-Platform SSH Client</strong>
</p>

<p align="center">
  <img src="static/app-icon.png" alt="Termust Logo" width="128" height="128" />
</p>

---

## Overview

**Termust** adalah aplikasi desktop SSH client yang modern, cepat, dan mengutamakan privasi (_local-first_). Dibangun dengan kombinasi performa native Rust dan antarmuka desktop yang responsif menggunakan Tauri v2 serta Svelte 5.

Semua data sensitif—mulai dari kredensial server, private key, riwayat host, hingga snippet perintah—disimpan terenkripsi secara lokal di perangkat Anda tanpa ketergantungan pada server cloud pihak ketiga.

---

## Fitur Utama

- **Encrypted Local Vault**: Semua data inventaris (Host, Kredensial, Grup) dienkripsi secara lokal menggunakan standar kriptografi modern (Argon2id + ChaCha20-Poly1305) dan dilindungi oleh Master Password.
- **Host & Inventory Management**: Kelola ratusan server dengan pengelompokan (Groups), tautan kredensial otomatis, dan koneksi satu kali klik tanpa perlu mengetik ulang password atau passphrase private key.
- **Multi-Session Terminal Tabs**: Jalankan beberapa sesi SSH sekaligus dalam satu jendela dengan manajemen tab yang terisolasi, riwayat _scrollback_ persisten per tab, dan indikator status koneksi _real-time_.
- **Quick Connect**: Koneksi instan ke server manapun tanpa harus menyimpannya ke inventaris terlebih dahulu.
- **Encrypted Snippets**: Simpan perintah atau skrip yang sering digunakan ke dalam vault, dengan dialog konfirmasi dan pratinjau sebelum dikirim ke terminal.
- **Import OpenSSH Config**: Impor konfigurasi host yang sudah ada dari file `~/.ssh/config` dengan mudah.
- **Persistent Known Hosts**: Verifikasi dan kelola fingerprint host key secara aman di dalam vault lokal.
- **Encrypted Backup & Restore**: Ekspor dan impor seluruh data inventaris ke file cadangan `.termust-backup` yang terenkripsi mandiri.
- **Cross-Platform**: Dukungan penuh untuk macOS, Linux, dan Windows.

---

## Teknologi

- **Core / Backend**: [Rust](https://www.rust-lang.org/) & [Tauri v2](https://tauri.app/)
- **SSH Engine**: [russh](https://github.com/warp-tech/russh) (dengan backend kripto `ring`)
- **Terminal Emulator**: [xterm.js](https://xtermjs.org/)
- **Frontend**: [Svelte 5](https://svelte.dev/) & [SvelteKit](https://kit.svelte.dev/)
- **Styling**: [Tailwind CSS](https://tailwindcss.com/) & [shadcn-svelte](https://shadcn-svelte.com/)
- **Database Lokal**: SQLite terenkripsi (Zeroize memory protection)

---

## Lisensi

Termust didistribusikan di bawah lisensi [MIT](LICENSE) atau [Apache-2.0](LICENSE).
