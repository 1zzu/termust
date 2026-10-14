# Format backup terenkripsi v1

Dokumen ini menjelaskan format file backup Termust, cara file itu ditulis dan dibaca, serta apa yang
terjadi saat restore. Format penyimpanan vault sendiri tidak berubah sama sekali: lihat
[`vault-format-v1.md`](vault-format-v1.md) dan [`local-data-model-v1.md`](local-data-model-v1.md).

Backup adalah **satu file** berisi salinan seluruh domain data vault. Backup **bukan** salinan
database: file tidak memuat metadata vault, tidak memuat wrapped Vault Key, dan tidak memuat satu byte
pun ciphertext dari vault sumber.

## Ekstensi dan ukuran

- Ekstensi file: `.termust-backup`.
- Nama default yang ditawarkan: `termust-backup-YYYY-MM-DD.termust-backup`.
- Batas ukuran file: 64 MiB (`MAX_BACKUP_BYTES`). Batas itu diperiksa dari **satu file handle**, yaitu handle
  yang sama yang dipakai untuk membaca isinya, lalu diterapkan lagi pada stream yang benar-benar dibaca: paling
  banyak `MAX_BACKUP_BYTES + 1` byte yang diambil dari handle itu. File yang diganti atau membesar setelah
  metadata diperiksa karena itu tidak dapat membuat proses mengalokasikan melebihi batas, dan file yang ternyata
  lebih panjang ditolak — bukan dipotong. Tidak ada `read` tanpa batas terhadap file backup.
- Batas payload setelah didekripsi: 64 MiB, dan batas jumlah object 50.000.

## Envelope

File adalah satu objek JSON. Bagian yang **plaintext** hanyalah kepala berikut:

```json
{
	"magic": "TERMUST_BACKUP",
	"formatVersion": 1,
	"kdf": {
		"algorithm": "argon2id",
		"salt": "<base64, 16 byte acak>",
		"memoryCost": 65536,
		"timeCost": 3,
		"parallelism": 4
	},
	"cipher": "xchacha20-poly1305",
	"nonce": "<base64, 24 byte acak>",
	"ciphertext": "<base64>"
}
```

Aturan pada kepala:

- `magic` dan `formatVersion` diperiksa lebih dulu; file yang bukan backup ditolak sebelum kunci
  apa pun diturunkan.
- Field yang tidak dikenal ditolak (`deny_unknown_fields`), termasuk pada objek `kdf`.
- Salt Argon2id dan nonce boleh plaintext: keduanya bukan rahasia, dan nonce terikat ke ciphertext
  oleh authentication tag.
- **Tidak ada** data domain di kepala: tidak ada jumlah object, tidak ada nama, username, address,
  fingerprint, nama atau isi snippet, dan tidak ada timestamp. Semuanya berada di dalam ciphertext.
- Salt dan nonce diundi ulang untuk setiap export, jadi dua export dari vault yang sama tidak berbagi
  key material maupun ciphertext.

## Enkripsi

| Bagian     | Algoritma                                            |
| ---------- | ---------------------------------------------------- |
| Kunci file | Argon2id dari password backup, salt 16 byte, 32 byte |
| Payload    | XChaCha20-Poly1305, nonce 24 byte, authenticated     |
| AAD        | `termust:backup:v1:payload` (UTF-8, byte demi byte)  |

AAD adalah sebuah konstanta tetap milik format backup, dan **versi formatnya tertulis di dalamnya**:
v1 memakai `termust:backup:v1:payload`. Versi itu bukan `cryptoVersion` vault — yang menjawab
pertanyaan berbeda, yaitu bagaimana sebuah object vault dienkripsi. Karena AAD-nya berbeda dari AAD
object (`termust` ‖ 0x00 ‖ crypto_version ‖ 0x00 ‖ `object` ‖ id ‖ kind) dan AAD wrapped Vault Key,
ciphertext backup tidak dapat diterima sebagai payload object atau sebagai wrapped Vault Key, dan
sebaliknya; karena `v1` ada di dalam AAD, ciphertext yang terikat ke format lain juga tidak dapat
dibuka sebagai v1, walaupun password, salt, dan nonce-nya sama.

Parameter KDF berasal dari file, jadi parameter itu **dibatasi lebih dulu**. `memoryCost` maksimum
256 MiB, `timeCost` maksimum 6, `parallelism` maksimum 8, dan tidak boleh nol atau di bawah batas
minimum Argon2id (memory 8 KiB, time 1, parallelism 1). Batas ini adalah **proteksi
resource-exhaustion untuk build ini, bukan parameter enkripsi default**: parameter produksi
(64 MiB / 3 / 4) jauh di bawahnya, dan batas ini hanya menutup file yang meminta biaya derivasi yang
tidak wajar untuk sebuah aplikasi desktop maupun target mobile. Parameter di luar rentang itu
ditolak sebelum Argon2id dijalankan sama sekali, dengan pesan yang berbeda dari "password salah" —
jadi file yang ditolak tidak pernah memakan 256 MiB atau satu core penuh.

## Isi ciphertext

Payload dideserialisasi **langsung dari buffer hasil dekripsi** ke tipe `BackupPayload`; tidak ada
tree `serde_json::Value` perantara yang menyimpan salinan plaintext tiap credential dan snippet.
Setelah payload terbentuk, `payloadVersion` dan metadata lainnya diperiksa. Bentuk payload:

```json
{
	"payloadVersion": 1,
	"createdAt": 1760000000,
	"sourceAppVersion": "0.1.0",
	"objects": [
		{
			"id": "<uuid>",
			"kind": "host",
			"payload": "<JSON record>",
			"cryptoVersion": 1,
			"createdAt": 1760000000,
			"updatedAt": 1760000000,
			"deletedAt": null
		}
	]
}
```

- `createdAt`, `sourceAppVersion`, jumlah object, jenis object, dan seluruh record domain berada di
  dalam ciphertext.
- Setiap object membawa `id`, `kind`, `payload` (record domain sebagai JSON: Group, Host, Credential,
  Known host, atau Snippet), `cryptoVersion`, timestamp, dan tombstone.
- Tidak ada byte ciphertext dari vault sumber. Yang disimpan adalah record domain-nya, dan record itu
  **dienkripsi ulang** memakai Vault Key milik vault tujuan saat restore. Karena itu backup yang sama
  dapat dipulihkan ke vault dengan Vault Key berbeda.

### Yang disertakan

Seluruh isi `vault_objects`: Group, Host, Credential (termasuk secret), Known host, Snippet, object ID,
object kind, payload version, crypto version, `createdAt`, `updatedAt`, dan `deletedAt` (tombstone).

### Yang tidak disertakan

Session SSH yang sedang berjalan, isi scrollback terminal, UI state, dialog state, log, cache,
temporary file, SQLite journal/WAL, master password vault, Vault Key, derived key, dan path spesifik
perangkat. Backup juga bukan sync: tidak ada merge, tidak ada incremental, dan tidak ada resolusi
konflik.

## Export

`export_encrypted_backup` menerima `{ destinationPath, backupPassword, overwrite }`:

1. Vault harus unlocked, dan password backup minimal 12 karakter.
2. Path harus punya nama file dan berada di direktori yang sudah ada. Backend tidak membuat direktori.
3. Jika file sudah ada dan `overwrite` tidak dikirim, export berhenti tanpa menulis apa pun. Flag itu
   hanya dikirim setelah dialog save native menanyakan penggantian file — jadi pemanggil yang tidak
   bertanya tidak dapat menghapus backup lama.
4. Seluruh object dibaca dari satu koneksi (satu snapshot konsisten), didekripsi, lalu **dibaca ulang
   dan divalidasi** dengan fungsi yang sama yang dipakai command lain.
5. Payload diserialisasi, dienkripsi, lalu ditulis secara atomik.

Export **fail closed**: object dengan kind yang tidak dikenal, payload version yang tidak didukung,
crypto version yang lebih baru, object yang rusak, atau data vault yang tidak konsisten membuat export
gagal seluruhnya. Tidak ada backup yang diam-diam kehilangan object yang tidak dipahami.

### Penulisan file atomik

Isi backup ditulis ke file temporary di direktori tujuan yang sama (dibuat dengan permission ketat
platform; mode `0600` di Unix), di-flush dan di-`sync` ke platform, lalu **dipublikasikan**:

- `overwrite = true` memakai replace move atomik — `rename` di Unix, `MoveFileExW` dengan
  `MOVEFILE_REPLACE_EXISTING` di Windows, karena `std::fs::rename` Windows menolak menimpa file yang
  sudah ada;
- `overwrite = false` memakai no-clobber move atomik (`RENAME_NOREPLACE`/hard link di Unix,
  `MoveFileExW` tanpa flag replace di Windows), sehingga file yang muncul **setelah** pengecekan awal
  pun tidak pernah diganti.

Keputusan mengganti atau tidak ada di filesystem, bukan di sebuah pemeriksaan `exists()` sebelum
rename, jadi tidak ada TOCTOU race antara check dan rename. Tujuan berisi file lama utuh atau file
baru utuh — tidak pernah setengah tertulis. File temporary dibersihkan oleh lifecycle/RAII milik
`tempfile` pada setiap jalur gagal maupun panic. File temporary selalu berisi envelope ciphertext:
tidak pernah ada plaintext yang ditulis ke disk, tidak ada plaintext temporary file, dan backup tidak
pernah masuk clipboard, `localStorage`, atau `sessionStorage`.

## Inspect

`inspect_encrypted_backup` menerima `{ sourcePath, backupPassword }` dan **tidak menulis apa pun**, baik
ke vault maupun ke disk. Command ini:

1. memastikan vault unlocked;
2. menolak file yang lebih besar dari batas, direktori, dan symbolic link (pemeriksaan symbolic link bersifat
   **best-effort**: pemeriksaan path dan `open` adalah dua langkah, jadi link yang muncul di antaranya tetap akan
   diikuti. Batas ukuran tidak bergantung pada itu, karena batas diterapkan pada stream yang benar-benar dibuka,
   bukan pada path);
3. membaca kepala, memvalidasi magic, format version, nama KDF dan nama cipher;
4. memvalidasi batas parameter KDF;
5. mendekripsi payload dan memvalidasi seluruh snapshot dengan cara yang sama seperti restore;
6. mengembalikan ringkasan saja:

```text
createdAt, sourceAppVersion, totalObjects, activeObjects, deletedObjects, countByKind, fileDigest
```

Tidak ada secret dan tidak ada payload utuh yang dikembalikan ke frontend. Preview hanya menunjukkan
berapa banyak datanya, bukan isinya.

`fileDigest` adalah `sha256:<base64>` dari byte file yang baru dibaca. Nilai itu bukan rahasia dan
bukan kunci; fungsinya adalah mengikat preview ke file yang nanti benar-benar dipakai restore.

## Restore

`restore_encrypted_backup` menerima `{ sourcePath, backupPassword, mode, confirmation, fileDigest }`.

- Field yang tidak dikenal ditolak.
- `mode` harus `"replace"`; satu-satunya mode yang ada di v1 adalah replace. Tidak ada merge,
  keep-local, keep-backup, per-object selection, atau conflict UI.
- `confirmation` harus `"RESTORE"` persis. Frontend meminta pengguna mengetiknya, dan backend
  memeriksanya sendiri: tombol yang disabled hanyalah kenyamanan, bukan pengaman.
- `fileDigest` harus sama dengan digest yang dilaporkan inspect untuk file itu. File yang berubah
  setelah di-preview — ditulis ulang, tersalin sebagian, atau ditukar dengan file lain — ditolak
  dengan pesan tersendiri sebelum kripto dijalankan. Restore tanpa digest hasil inspect juga ditolak.
- Restore **ditolak** bila ada session SSH yang aktif atau sedang connecting. Aplikasi tidak pernah
  memutus session itu sendiri, dan selama restore berjalan **session baru tidak dapat dibuat**:
  pemeriksaan session dan pengambilan guard dilakukan dalam satu critical section pada state yang
  sama (`SshState::begin_maintenance`). Guard itu hidup selama seluruh operasi restore, termasuk
  pekerjaan blocking dan dekripsi, dan **dimiliki oleh pekerjaan blocking itu sendiri**, bukan oleh
  future command: command yang dibatalkan (future-nya di-drop) tidak melepas guard selama restore masih
  mengubah vault, karena task blocking yang sudah berjalan tetap berjalan sampai selesai. Guard dilepas
  lewat RAII pada sukses, error, early return, maupun panic. Guard hanya menahan registration; ia tidak
  menahan lock saat I/O atau kripto berjalan.
- File harus dipilih pengguna; path tidak pernah diambil dari isi backup, dan referensi file lain
  tidak pernah diikuti.

Urutan yang wajib dipatuhi: vault unlocked → mode dan confirmation → ambil restore guard (yang juga
memeriksa tidak ada session aktif) → baca file dengan batas ukuran pada stream → cocokkan `fileDigest` → validasi
envelope dan parameter KDF → dekripsi → validasi seluruh snapshot → siapkan seluruh object untuk
dienkripsi ulang → **satu transaksi SQLite** → commit hanya bila semuanya berhasil.

Restore tidak parsial. Bila ada satu langkah yang gagal sebelum transaksi dimulai, vault tidak tersentuh
sama sekali. Di dalam transaksi, `DELETE FROM vault_objects` dan seluruh `INSERT` adalah satu unit
kerja: bila salah satunya gagal, seluruh transaksi di-rollback dan tabel kembali seperti semula.
Restore tidak memakai loop command CRUD publik.

Setelah commit, frontend membaca ulang seluruh inventory: Groups, Hosts, Credential, Known hosts, dan
Snippets. Bila restore gagal, data saat ini tetap ada, inventory tidak dikosongkan, dan tidak ada
klaim sukses.

### Vault tujuan tetap dipakai

Restore mempertahankan vault tujuan beserta seluruh identitasnya:

- master password vault tujuan;
- Vault Key vault tujuan;
- metadata vault tujuan (`vault_meta` tidak pernah disentuh);
- parameter Argon2 unlock vault tujuan.

Backup password hanya membuka file backup. Backup password **tidak pernah** menjadi master password
baru, dan master password vault sumber tidak diperlukan untuk restore.

### Validasi snapshot sebelum transaksi

Sebelum satu baris pun diubah, seluruh snapshot diperiksa:

- magic, format version, nama KDF dan nama cipher benar;
- parameter KDF berada dalam batas aman;
- authentication tag valid;
- payload hasil dekripsi di bawah batas ukuran, jumlah object di bawah batas wajar (50.000);
- metadata payload valid: `payloadVersion` didukung, `createdAt` tidak negatif, dan `sourceAppVersion`
  tidak kosong serta tidak melebihi 64 karakter;
- seluruh ID non-empty dan unik;
- seluruh kind dikenal (Group, Host, Credential, Known host, Snippet);
- payload version dan crypto version didukung;
- timestamp valid dan konsisten (`updatedAt` tidak mendahului `createdAt`, `deletedAt` tidak mendahului
  `updatedAt`), dan timestamp di dalam record memenuhi keempat invariant berikut: `record.createdAt ≥ 0`,
  `record.updatedAt ≥ record.createdAt`, `record.createdAt ≤ object.createdAt`, dan
  `record.updatedAt ≤ object.updatedAt`. Record dengan timestamp negatif atau yang lebih tua dari
  createnya sendiri tidak dapat dibuat lewat CRUD, jadi tidak dapat masuk lewat restore. Equality dan
  kesamaan tidak diwajibkan — timestamp internal yang **lebih lama** daripada row adalah kasus normal,
  karena command menulis row dengan `now()`-nya sendiri sesaat setelah record dan tombstone memajukan row
  tanpa menulis ulang payload;
- setiap record dapat didekode oleh fungsi yang sama dengan command biasa, **dan** lolos aturan field
  yang sama dengan CRUD normal: nama/label/address/username wajib ada dan tidak melebihi batasnya,
  nilai yang akan di-trim command harus sudah ter-trim, port 1–65535, nama group/credential/snippet,
  secret credential tidak kosong dan tidak melebihi 16 KiB, passphrase kosong ditolak, isi snippet
  tidak kosong, di bawah 64 KiB, dan tanpa NUL, serta fingerprint/key algorithm known host tidak
  kosong dan tidak melebihi batasnya;
- tombstone valid, dan tidak ada object yang diam-diam dilewati;
- `id` di dalam payload cocok dengan `id` object pembungkusnya;
- Host `groupId` mengarah ke Group aktif yang ada, atau `null`; bentuk string kosong ditolak karena
  command normal menyimpannya sebagai `null`;
- Host `credentialId` mengarah ke Credential aktif yang ada, atau `null`;
- tidak ada dua Group aktif dengan nama yang sama tanpa memperhatikan kapitalisasi;
- tidak ada dua Known host aktif dengan `hostKeyId` yang sama.

Data yang tidak valid **ditolak**, bukan dinormalisasi: restore tidak pernah memperbaiki atau
mengubah isi record diam-diam, sehingga vault hasil restore selalu berisi data yang juga bisa dibuat
lewat UI.

### Tombstone

Backup menyimpan tombstone agar snapshot tetap setia:

- object aktif tetap aktif setelah restore;
- object bertombstone tetap bertombstone;
- tombstone tidak pernah muncul di daftar aktif;
- timestamp tombstone dipertahankan;
- Host, Credential, Group, Known host, atau Snippet yang sudah dihapus tidak pernah dihidupkan kembali.

Restore mengganti **seluruh** isi vault: record yang tidak ada di backup juga hilang, termasuk record
yang dibuat setelah backup itu dibuat.

## Password backup

- Hanya diminta saat export dan saat membuka file, tidak pernah disimpan, tidak masuk log, error,
  analytics, atau nama file.
- Frontend meminta dua kali saat export dan keduanya harus sama; minimum 12 karakter; backend
  memeriksa panjangnya lagi.
- Aplikasi tidak pernah diam-diam memakai master password vault sebagai password backup. Pengguna
  boleh memilih password yang sama, tetapi aplikasi tidak mengetahuinya.
- Buffer password, key turunan, dan plaintext payload di backend di-zeroize: buffer hasil dekripsi
  dibungkus `Zeroizing`, payload tiap record memakai `Zeroizing<String>`, dan serialisasi envelope
  object juga `Zeroizing`. Tidak ada `serde_json::Value` perantara yang menyimpan salinan plaintext
  credential/snippet.
- `Debug` untuk request yang memuat password tidak ada sama sekali (begitu juga untuk payload
  record, yang di-redact). Zeroization ini berlaku untuk buffer yang dimiliki aplikasi; Rust dan
  library di bawahnya tidak menjamin penghapusan setiap salinan sementara (misalnya di dalam
  allocator atau stack frame library kripto), dan itu tidak diklaim.
- Password di frontend dibuang saat selesai, cancel, error final, atau vault dikunci.

Password backup yang hilang berarti file itu tidak dapat dibuka lagi: tidak ada reset dan tidak ada
recovery key.

## Password salah dan file rusak

Password salah, tag authentication yang rusak, nonce yang diubah, ciphertext yang diubah, dan file
yang terpotong menghasilkan **satu pesan generik yang sama**:

```text
The backup could not be opened. The password is wrong or the file is damaged.
```

Tidak ada password oracle dan tidak ada detail crypto yang dibocorkan. Kondisi yang memang dapat
ditindaklanjuti pengguna tetap dibedakan, misalnya format version yang tidak didukung, KDF yang tidak
dikenal, cipher yang tidak dikenal, file yang bukan backup, dan file yang terlalu besar.

## Versi mendatang

- `formatVersion` dan `payloadVersion` bergerak sendiri-sendiri. Build ini hanya menulis dan membaca
  versi 1, dan memakai AAD `termust:backup:v1:payload` untuk itu.
- Format version yang baru wajib memakai AAD-nya sendiri (`termust:backup:v2:payload`, atau bentuk
  lain yang terikat versi): ciphertext v1 tidak akan terbuka di build yang mengharapkan AAD lain,
  jadi versi tidak dapat tertukar tanpa terdeteksi. Satu test mengikat literal versi di AAD ke
  `FORMAT_VERSION` supaya keduanya tidak bisa berbeda diam-diam.
- File dengan format version, payload version, crypto version, atau kind object yang lebih baru
  ditolak dengan pesan yang jelas; build ini tidak mencoba membaca sebagian.
- Field yang tidak dikenal ditolak, bukan diabaikan.
- `fileDigest` adalah detail internal build ini (SHA-256 dari byte file), bukan bagian dari format
  file dan tidak disimpan di dalam file; ia hanya mengikat satu preview ke satu restore.

## Batas pemulihan

- Backup bukan sync, bukan cloud backup, dan tidak otomatis: hanya dibuat dan dipulihkan saat pengguna
  memintanya.
- Backup hanya memuat domain data. Master password, Vault Key, dan parameter unlock vault **tidak**
  ikut, sehingga file tidak dapat dipakai untuk membuka vault yang terkunci tanpa passwordnya.
- Restore bersifat destruktif dan tidak dapat dibatalkan kecuali pengguna memiliki backup lain.
- Backup yang dibuat sebelum sebuah kind object ada tetap dapat dipulihkan: object yang tidak ada di
  file tidak akan muncul kembali.
