# Basic OpenSSH config import v1

Dokumen ini menjelaskan import Host dari subset dasar OpenSSH client config pada Phase 1D:
sintaks yang dibaca, sintaks yang sengaja tidak dibaca, aturan duplicate, batas ukuran,
dan langkah memakainya.

**Ini bukan parser OpenSSH yang kompatibel penuh.** Yang diimplementasikan adalah subset kecil
yang benar dan jujur, dan setiap directive di luar subset itu dilaporkan, bukan ditebak.

## Yang didukung

```sshconfig
Host production
    HostName 192.0.2.10
    User deploy
    Port 22
```

- Directive: `Host`, `HostName`, `User`, `Port`. Nama directive tidak peka huruf besar/kecil.
- Whitespace awal dan whitespace antar-token boleh bervariasi. `Keyword=Value` dibaca sama
  seperti `Keyword Value`.
- Baris kosong diabaikan. Baris yang karakter non-whitespace pertamanya `#` adalah komentar.
  Komentar di akhir baris directive **tidak** didukung — OpenSSH pun tidak mendukungnya — jadi
  nilai seperti `192.0.2.10 # production` ditolak dengan alasan, bukan dipangkas diam-diam.
- Satu `Host` block hanya dipakai bila pattern-nya tepat satu alias literal.
- Alias menjadi `label` Host. Bila `HostName` tidak ada, alias juga menjadi `address`.
- Bila `Port` tidak ada, dipakai `22`. Port harus berada pada rentang 1..65535.
- `User` wajib ada, karena model Host Termust tidak punya username default. Block tanpa `User`
  tetap tampil di preview sebagai `invalid`.
- Bila satu directive muncul lebih dari sekali dalam satu block, nilai pertama yang dipakai dan
  kemunculan berikutnya menghasilkan catatan — nilai kedua tidak pernah menimpa yang pertama.
- Nilai `HostName` dan `User` dipakai apa adanya setelah trim. Tidak ada DNS lookup, tidak ada
  koneksi jaringan, tidak ada normalisasi.

Contoh config yang seluruhnya dapat diimpor:

```sshconfig
# Production
Host production
    HostName 192.0.2.10
    User deploy

Host staging
	HostName=192.0.2.20   # tidak dipakai: komentar di akhir baris ditolak
	User deploy
	Port 2222
```

## Yang sengaja tidak didukung

Semuanya dilaporkan sebagai catatan pada barisnya, dan tidak pernah ditafsirkan diam-diam:

| Bentuk                                                                                      | Perlakuan                                                                                |
| ------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| `Host *`, wildcard `*` atau `?`                                                             | Entry `unsupported`, tidak diimpor                                                       |
| Pattern negasi `!host`                                                                      | Entry `unsupported`, tidak diimpor                                                       |
| Lebih dari satu alias pada satu `Host`                                                      | Entry `unsupported`, tidak diimpor                                                       |
| `Match`                                                                                     | Section dilewati sampai `Host` berikutnya, dengan satu catatan                           |
| `Include`                                                                                   | Tidak dibuka, tidak diikuti, file yang direferensikan tidak pernah dibaca                |
| `IdentityFile`                                                                              | Path key tidak dibaca, tidak disimpan, dan tidak menjadi Credential                      |
| `CertificateFile`, `IdentityAgent`                                                          | Tidak dibaca, dilaporkan                                                                 |
| `PreferredAuthentications`                                                                  | Dilaporkan; autentikasi memakai Credential yang ditautkan ke Host                        |
| `ProxyJump`, `ProxyCommand`                                                                 | Dilaporkan dengan jelas: tidak ada jump host dan tidak ada command lokal yang dijalankan |
| `LocalForward`, `RemoteForward`, `DynamicForward`                                           | Dilaporkan; port forwarding belum ada                                                    |
| Token expansion (`%h`, `%p`, `%r`) dan environment/command expansion (`$HOME`, `` `cmd` ``) | Nilainya tidak dapat dipakai, entry menjadi `invalid` atau `unsupported`                 |
| Quoting (`"deploy"`)                                                                        | Tidak diinterpretasikan; nilainya ditolak dengan alasan                                  |
| Line continuation (`\` di akhir baris)                                                      | Baris itu diabaikan dengan catatan                                                       |
| Global default di luar block `Host`                                                         | Diabaikan dengan catatan; tidak diterapkan ke block mana pun                             |

Konsekuensi yang penting: **precedence OpenSSH tidak ditiru.** Tidak ada `Host *` default, tidak
ada penggabungan nilai antar block, dan tidak ada nilai yang diwarisi. Setiap block berdiri sendiri.

## Preview

`preview_openssh_config` menerima `{ content }` dan hanya membaca:

1. Vault harus unlocked.
2. Ukuran content divalidasi (maksimum 1 MiB).
3. Content diparse oleh parser yang sama dengan yang dipakai import.
4. Host yang sedang aktif di vault dibaca untuk deteksi duplicate.
5. Preview dikembalikan.

Preview **tidak menulis apa pun**: tidak ada Host, Credential, Group, known host, atau tombstone
yang dibuat, dan tidak ada satu byte pun yang berubah di database. Content config dikirim sebagai
teks, bukan path, jadi tidak ada file yang dibuka selain file yang dipilih pengguna di frontend.

Status tiap entry:

- `ready` — dapat diimpor, dan dipilih secara default di UI.
- `already_exists` — sudah ada Host aktif dengan identity yang sama. Tidak dipilih, tidak dapat
  dipilih.
- `unsupported` — memakai pattern yang tidak dibaca build ini. Tidak dipilih, tidak dapat dipilih.
- `invalid` — tidak dapat menjadi Host: `User` tidak ada, port tidak dapat dipakai, atau ada nilai
  lain yang ditolak. Tidak dipilih, tidak dapat dipilih.

Catatan membawa `line` (1-based) bila catatan itu tentang satu baris, dan `null` bila tentang
config secara keseluruhan. Pesan catatan dan pesan error tidak pernah mengulang nilai dari config.

## Import

`import_openssh_hosts` menerima `{ content, selectedEntryIndexes }` dan:

1. Memastikan vault unlocked.
2. Memvalidasi ukuran.
3. Memparse ulang content dengan parser yang sama — bukan memakai hasil preview yang dikirim
   frontend.
4. Memvalidasi seluruh index: setiap index harus menunjuk entry yang ada, dan tidak boleh ada
   index duplikat. Index yang tidak valid menolak seluruh permintaan sebelum ada yang ditulis.
5. Hanya memproses entry berstatus `ready`.
6. Menjalankan deteksi duplicate sekali lagi.
7. Membuat Host dengan `label` dari alias, `address` dari `HostName` atau alias, `port` hasil parse
   atau 22, `username` dari `User`, `groupId = null`, dan `credentialId = null`.
8. Mengembalikan hasil per entry: `imported`, `skipped`, atau `failed`, beserta alasannya.

Yang **tidak** dilakukan import: membuat Credential, membuat Group, mengubah atau menimpa Host yang
sudah ada, menghapus apa pun, menulis riwayat import, atau menyimpan potongan config OpenSSH di
payload Host. Host hasil import adalah Host biasa: terenkripsi di `vault_objects` dengan `kind`
`host`, muncul di daftar Host, dapat diedit, dapat diberi Group dan Credential, dan tidak punya
penanda “imported”. Model data tidak berubah; lihat
[`local-data-model-v1.md`](local-data-model-v1.md).

### Partial success

Import **tidak atomic**. Setiap Host ditulis satu per satu lewat service Host yang sama, sehingga
satu entry yang gagal tidak membatalkan entry yang sudah ditulis. Hasil per entry selalu dilaporkan
apa adanya di UI, dan dialog tidak menutup sendiri selama ada entry yang perlu dibaca.

Mengulang import yang sama aman: duplicate detection dijalankan kembali, jadi Host yang sudah ada
dilaporkan `skipped` dan tidak dibuat dua kali.

## Aturan duplicate

Sebuah entry dianggap sudah ada bila terdapat Host **aktif** dengan identity koneksi yang sama:

```text
normalized address + port + username
```

Normalisasinya kecil dan disengaja:

- `address` dan `username` di-trim.
- Hostname dibandingkan tanpa memandang huruf besar/kecil.
- IP literal dibandingkan persis seperti yang tertulis — `[2001:db8::1]` dan `2001:db8::1` tidak
  disatukan, dan alamat tidak ditulis ulang ke bentuk lain.
- Username dibandingkan peka huruf besar/kecil.
- `port` dibandingkan sebagai angka.

Tombstone (Host yang dihapus) tidak dianggap Host aktif, jadi target yang pernah dihapus dapat
diimpor lagi. Label yang sama dengan target berbeda bukan duplicate: entry tetap `ready`, Host baru
ditambahkan di sampingnya, dan Host lama tidak pernah ditimpa. Preview memberi catatan bahwa label
itu sudah dipakai.

## Batas ukuran

Maksimum **1 MiB** per config, dihitung dalam byte (frontend memakai `TextEncoder`, backend memakai
`len()` dari string UTF-8). Frontend memvalidasi sebelum mengirim — dan memakai ukuran byte file
yang sudah diketahui saat config dipilih dari file — tetapi backend memvalidasi ulang, karena
validasi frontend bukan jaminan. Config kosong atau hanya whitespace ditolak sebelum dikirim.
Content config tidak pernah masuk `localStorage`, tidak pernah masuk log, dan dibuang dari state
dialog saat dialog ditutup, saat import selesai, atau saat vault dikunci.

## Batas keamanan

Config OpenSSH diperlakukan sebagai input tidak terpercaya:

- Tidak ada eksekusi proses lokal, tidak ada shell, tidak ada `Command`.
- Tidak ada shell expansion, environment expansion, atau command substitution.
- Tidak ada DNS lookup dan tidak ada koneksi jaringan saat preview atau import.
- Tidak ada pembacaan `IdentityFile`, `CertificateFile`, `IdentityAgent`, atau `Include`.
- Backend tidak membuka path apa pun dari config, jadi tidak ada path traversal.
- Parser tidak panic pada input arbitrer (diuji dengan loop deterministik atas ribuan input acak).
- Parser tidak membaca file lain dan tidak pernah dijalankan dengan lock vault dipegang saat
  menulis.
- Alias dirender sebagai teks biasa; tidak ada `{@html}` di UI.

## Memakai import dari UI

1. Unlock vault.
2. Buka section **Hosts**, tekan **Import OpenSSH config**.
3. Pilih file config (file dibaca di dalam window; path tidak dikirim ke backend) atau tempel isi
   config. Ukuran dan isi kosong divalidasi sebelum dikirim.
4. Tekan **Preview**. Tidak ada yang ditulis pada langkah ini.
5. Periksa daftar entry: alias, `user@address:port`, status, dan catatan per baris. Entry `ready`
   terpilih otomatis; entry lain tidak dapat dipilih. Hilangkan centang pada entry yang tidak ingin
   diimpor.
6. Tekan **Continue** untuk melihat konfirmasi: jumlah Host yang akan dibuat, bahwa Host tersebut
   belum punya Credential, dan bahwa directive yang tidak didukung tidak diimpor. Membatalkan di
   langkah ini tidak menulis apa pun.
7. Tekan **Import**. Hasilnya menampilkan jumlah `imported`, `skipped`, dan `failed`, plus alasan
   untuk tiap entry yang tidak diimpor.
8. Host baru muncul di daftar Host dengan `Credential required`. Tekan **Edit** untuk memilih
   Credential, lalu **Connect**.

## Menjalankan test

```sh
cd src-tauri
cargo test openssh              # parser, preview, import, duplicate, database
cargo test --lib                # seluruh test Rust, termasuk kontrak IPC
```

```sh
npm test                        # helper frontend + kontrak IPC
```

Test live yang membuktikan Host hasil import benar-benar dapat dipakai connect:

```sh
npm run test:ssh-integration
```

Test itu menyimpan vault sementara, mengimpor config yang menunjuk server Docker sekali pakai,
memastikan Host hasil import belum punya Credential, membuat Credential lewat service normal,
menautkannya ke Host itu, lalu membuka shell dan menjalankan command marker.
