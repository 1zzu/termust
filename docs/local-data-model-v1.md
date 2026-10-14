# Model data lokal v1

Dokumen ini menjelaskan Group, Host, dan Credential pada Phase 1B1, Snippet pada Phase 1C, serta Host
hasil import OpenSSH config pada Phase 1D: bentuk data, object kind, versi payload, relasi, dan
aturan delete. Format penyimpanan dan crypto-nya tidak berubah dan tetap dijelaskan di
[`vault-format-v1.md`](vault-format-v1.md).

## Tempat penyimpanan

Tidak ada tabel baru. Setiap record adalah satu row `vault_objects` dengan salah satu `kind`:

| Record     | `kind`       |
| ---------- | ------------ |
| Group      | `group`      |
| Host       | `host`       |
| Credential | `credential` |
| Known host | `known_host` |
| Snippet    | `snippet`    |

`kind` ikut menjadi bagian Additional Authenticated Data, jadi sebuah object tidak dapat dibaca
sebagai kind lain: ciphertext group tidak akan pernah bisa diparse sebagai Host. Enkripsi, nonce,
dan tombstone memakai jalur Phase 1A yang sama, satu object per record.

## Payload version

Setiap payload terenkripsi memiliki `version` (`1` saat ini). Payload dengan versi yang tidak
dikenal ditolak dengan pesan yang jelas, bukan dibiarkan lewat. Payload disimpan sebagai JSON
dengan nama field camelCase; field `version`, `id`, `createdAt`, dan `updatedAt` ada di setiap
record.

## Group

```json
{ "version": 1, "id": "…", "name": "Work", "createdAt": 1760000000, "updatedAt": 1760000000 }
```

- `name` wajib setelah trim dan maksimum 100 karakter.
- `name` unik tanpa memandang huruf besar/kecil, supaya daftar tidak pernah memuat dua nama yang
  terlihat sama.

## Host

```json
{
	"version": 1,
	"id": "…",
	"label": "web-1",
	"address": "web-1.example.com",
	"port": 22,
	"username": "root",
	"groupId": "… atau null",
	"credentialId": "… atau null",
	"createdAt": 1760000000,
	"updatedAt": 1760000000
}
```

- `label` (maks 100), `address` (maks 255), dan `username` (maks 100) wajib setelah trim.
- `port` harus berada pada rentang 1..65535.
- Tidak ada tags, icon, color, notes, jump host, forwarding, atau startup command pada fase ini.

## Credential

```json
{
	"version": 1,
	"id": "…",
	"name": "Prod",
	"secret": { "kind": "password", "password": "…" },
	"createdAt": 1760000000,
	"updatedAt": 1760000000
}
```

Jenis secret:

| `kind`        | Field                                  |
| ------------- | -------------------------------------- |
| `password`    | `password`                             |
| `private_key` | `private_key`, `passphrase` (opsional) |

- `name` wajib dan maksimum 100 karakter; secret wajib dan tidak boleh kosong; panjang secret
  dibatasi 16 KiB.
- Secret disimpan apa adanya tanpa trim, karena password dapat memuat spasi di ujungnya.
- Tidak ada SSH agent, certificate, hardware key, atau key path reference pada fase ini.

### Secret tidak pernah kembali ke frontend

Frontend hanya menerima `CredentialSummary`:

```text
CredentialSummary { id, name, kind, createdAt, updatedAt }
```

Tidak ada field secret pada tipe itu, sehingga secret tidak dapat dikirim kembali. Konsekuensinya:

- Mengganti nama credential tidak menyentuh secret-nya.
- Form edit tidak pernah memuat secret lama; secret hanya dapat dibuat atau diganti dengan nilai
  baru.
- Mengganti secret berarti mengirim nilai baru, bukan membaca yang lama.

## Known host

Satu SSH host key yang pernah disetujui pengguna untuk satu target.

```json
{
	"version": 1,
	"id": "…",
	"hostKeyId": "example.com:22",
	"keyAlgorithm": "ssh-ed25519",
	"fingerprintSha256": "SHA256:…",
	"createdAt": 1760000000,
	"updatedAt": 1760000000
}
```

- `hostKeyId` adalah identitas kanonik target, dan sekaligus kunci keunikan: hanya ada satu known
  host aktif untuk satu identitas. Menyetujui host yang sama dua kali menulis ulang record yang
  sudah ada, bukan menambah record kedua.
- `keyAlgorithm` dan `fingerprintSha256` adalah pasangan yang dibandingkan saat connect. Keduanya
  disimpan di dalam payload terenkripsi, jadi nama host dan fingerprint tidak pernah muncul sebagai
  plaintext di database.
- Public key mentah tidak disimpan: algoritma dan fingerprint sudah cukup untuk menolak key yang
  berbeda.

### Identitas kanonik `host_key_id`

Bentuknya `<host>:<port>`, dengan port selalu ikut sehingga host yang sama di port berbeda adalah
identitas berbeda. Sebelum dipakai, nama host di-trim, hurufnya dijadikan kecil (DNS tidak
membedakan huruf besar/kecil), dan tanda kurung siku di sekelilingnya dilepas. Literal IPv6 diberi
kurung siku seperti URL, sehingga `::1` pada port 22 menjadi `[::1]:22` dan tidak dapat terbaca
sebagai host `::1:22`.

### Kapan approval berlaku

| Keadaan vault                | Yang terjadi                                                                                                                   |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| Unlocked, host belum dikenal | Prompt muncul; menerima menyimpan known host ke vault.                                                                         |
| Unlocked, host sudah dikenal | Key dibandingkan; cocok berarti connect tanpa prompt.                                                                          |
| Locked                       | Record tidak dapat dibaca, jadi host dianggap belum dikenal dan prompt muncul; approval hanya berlaku sampai aplikasi ditutup. |
| Key berbeda                  | Koneksi diblokir, fingerprint lama dan baru ditampilkan, dan key baru tidak disimpan.                                          |

Key yang berubah tidak pernah diganti otomatis dan tidak ada tombol untuk menerimanya. Untuk
trust key baru, pengguna harus melupakan known host lama lebih dulu, lalu menyetujui key yang
baru saat connect berikutnya.

## Relasi

Relasi disimpan sebagai id, bukan salinan data:

- `Host.groupId` menunjuk satu Group aktif, atau `null`.
- `Host.credentialId` menunjuk satu Credential aktif, atau `null`.

Relasi diperiksa di service layer **setelah** payload didekripsi, karena vault sendiri tidak dapat
melihat isi object:

- Host tidak dapat dibuat atau diubah agar menunjuk record yang tidak ada atau sudah dihapus.
- Group yang masih dipakai Host tidak dapat dihapus.
- Credential yang masih dipakai Host tidak dapat dihapus.

Tidak ada cascade delete: record yang masih dipakai harus dipindahkan atau dihapus lebih dulu.

## Membaca Host dan Credential untuk koneksi (Phase 1B2b)

Sesi SSH dari saved Host dibaca di satu tempat, `credential_for_host`, dan hanya dari sana:

- Host dibaca berdasarkan id, lalu `address`, `port`, dan `username`-nya diperiksa ulang. Record
  yang tidak lengkap — misalnya ditulis build lain — ditolak sebelum ada koneksi jaringan.
- Host tanpa `credentialId`, Host yang sudah dihapus, dan Credential yang sudah dihapus masing-masing
  punya pesan error sendiri. Vault yang terkunci menolak pada langkah pertama.
- Payload `host` dan `credential` yang rusak ditolak sebagai corrupt, tanpa mengulang plaintext-nya.
- Hasilnya langsung menjadi request internal sesi: `address`, `port`, dan `username` dari Host, secret
  dari Credential. Secret tidak pernah menjadi bagian dari jawaban command dan tidak pernah dikirim
  ke frontend; byte-nya di-zeroize saat sesi selesai. Tidak ada field baru pada payload `host`
  atau `credential`, jadi format object tidak berubah.

## Snippet (Phase 1C)

Satu perintah SSH pendek yang disimpan terenkripsi.

```json
{
	"version": 1,
	"id": "…",
	"name": "Restart app",
	"content": "systemctl restart app",
	"createdAt": 1760000000,
	"updatedAt": 1760000000
}
```

- `name` wajib setelah trim dan maksimum 120 karakter.
- `content` wajib, panjangnya 1 byte sampai 64 KiB UTF-8, dan tidak boleh memuat NUL byte.
- `content` disimpan apa adanya: tidak di-trim, tidak dinormalisasi, quoting dan baris barunya utuh.
  Termust tidak memparse sintaks shell, jadi tidak ada validasi yang bergantung pada arti command.
- `name` tidak harus unik; `id` adalah identitas sebenarnya.
- Snippet tidak punya relasi ke Host, Group, maupun Credential. Payload v1 tidak punya tags, folder,
  variables, atau secret field.
- Daftar aktif diurutkan berdasarkan `name` (tanpa memandang huruf besar/kecil) lalu `id`, sehingga
  urutannya stabil di setiap pembacaan.
- Delete memakai tombstone yang sama: snippet yang dihapus hilang dari daftar aktif dan tidak dapat
  dibaca atau dijalankan lagi.

### Snippet bukan Credential

Isi snippet terenkripsi seperti object lain, tetapi snippet **bukan** secret store: isinya dikirim ke
shell jarak jauh sebagai input biasa dan akan terlihat di terminal. Password, token, dan private key
tetap milik Credential; secret variables adalah fitur terpisah di masa depan.

### Menjalankan snippet

Frontend hanya mengirim `sessionId` dan `snippetId`; isi diselesaikan backend dari vault. Sebelum
eksekusi, UI selalu menampilkan preview lengkap dan meminta konfirmasi, dan tidak ada eksekusi otomatis
— tidak saat snippet dibuat, diedit, didaftar, saat vault dibuka, maupun saat koneksi dibuat.

Content dikirim ke PTY session SSH yang dipilih memakai primitive input yang sama dengan keyboard
terminal, bukan shell lokal atau process lokal apa pun. Satu karakter Enter dikirim sebagai input
terpisah setelah isi, dan tidak ditambahkan bila isi sudah berakhir dengan baris baru, supaya snippet
seperti `echo one` dan `echo one\n` berperilaku sama: tepat satu submit. Jika tidak ada session yang
aktif, command ditolak dan tidak ada yang dikirim.

## Host hasil import OpenSSH (Phase 1D)

Import OpenSSH config tidak menambahkan jenis record, tabel, atau payload version baru. Import
hanya **membuat Host biasa** lewat service Host yang sama, dengan `kind` `host` dan payload v1 yang
sama seperti Host yang dibuat dari form:

- `label` = alias dari directive `Host`.
- `address` = `HostName`, atau alias bila `HostName` tidak ada.
- `port` = `Port`, atau 22 bila tidak ada.
- `username` = `User`; block tanpa `User` tidak dapat diimpor.
- `groupId` dan `credentialId` selalu `null`. Import tidak pernah membuat atau menautkan Credential
  dan Group.

Tidak ada penanda asal, tidak ada tabel import history, dan tidak ada potongan sintaks OpenSSH yang
disimpan di payload Host. Karena itu Host hasil import dapat diedit, diberi Group, ditautkan ke
Credential, dan dihubungkan lewat jalur saved Host yang sama seperti Host lainnya. Preview, status
entry, dan aturan duplicate-nya dijelaskan di
[`openssh-import-v1.md`](openssh-import-v1.md).

## Delete

Semua delete memakai tombstone: row tetap ada dan `deleted_at` diisi. Object yang sudah dihapus
hilang dari daftar aktif dan tidak dapat dibaca lagi.

## Yang belum ada

Multi-sesi/multi-tab, reconnect otomatis, session restore, auto-run command, snippet variables,
snippet tags, snippet folders, snippet history, snippet sharing/sync, SSH agent, certificate,
keyboard-interactive, credential prompt fallback, tags, notes, search, import `known_hosts` OpenSSH,
parser OpenSSH lengkap (`Match`, `Include`, wildcard, `Host *` default), import key dari config,
menulis config OpenSSH, wildcard host, jump host, backup, auto-lock, dan sinkronisasi berada di luar
fase ini.
