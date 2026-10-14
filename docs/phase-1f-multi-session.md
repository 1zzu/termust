# Multi-session terminal tabs (Phase 1F)

Dokumen ini menjelaskan model tab multi-sesi dan memuat checklist yang hanya bisa dibuktikan oleh
manusia di depan aplikasi Tauri yang benar-benar berjalan. Unit test dan integration test **tidak**
menggantikan item di sini, dan tidak ada item yang boleh ditandai lulus hanya karena test hijau.

## Model

Satu window memegang beberapa sesi SSH sekaligus, satu tab untuk setiap sesi.

```text
+page.svelte            daftar TerminalSession + activeSessionId, form, render tab dan panel
  session-tab-strip      tab list: status dot, title, tombol close, keyboard
  terminal-session       satu instance Terminal + FitAddon + container + ResizeObserver per tab
session-tabs.ts         tipe descriptor + fungsi state murni (dibuat/diuji tanpa DOM)
session-start.ts        lifecycle satu start: queued/invoking/finished, cancellable, punya done
session-lifecycle.ts    subscribe/start/close/disconnect/teardown, dan urutannya
ssh.ts                  IPC + decodeOutput (routing key: sessionId)
```

Descriptor tab (`TerminalSession`) hanya memuat `id`, `title`, `source`, `savedHostId?`, `status`,
`statusMessage`, `errorMessage`, `hostKeyPrompt`, `disconnecting`, dan `closing`. Tidak ada password,
private key, passphrase, atau secret apa pun di dalamnya: kredensial Quick Connect hidup di form
sampai request diserahkan ke backend, lalu dibersihkan, dan credential saved Host tetap diselesaikan
backend dari vault.

Empat keadaan tab dibedakan dengan tegas:

| Keadaan       | Arti                                                                                                                                               |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| connected     | Sesi hidup dan menerima input. Restore ditolak.                                                                                                    |
| disconnecting | Koneksi sedang dihentikan tetapi tab tetap ada. Input/resize ditolak, tombol Disconnect mati, `hasLiveSessions` tetap true sampai backend selesai. |
| disconnected  | Backend sudah benar-benar berhenti. Tab dan scrollback tetap ada; restore boleh dimulai.                                                           |
| closing       | Tab akan dihapus. Sesi dihentikan lebih dulu, dan tab baru hilang setelah stop berhasil.                                                           |

## Lifecycle

| Langkah          | Yang terjadi                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| create           | Form Quick Connect atau tombol Connect saved Host membuat satu `TerminalSession` berstatus `connecting`, menambahkannya ke daftar, dan menjadikannya aktif.                                                                                                                                                                                                                                                                                     |
| ready            | Tab melaporkan terminalnya siap; baru setelah itu `connect_ssh` / `start_saved_ssh` dipanggil, sehingga event awal tidak hilang.                                                                                                                                                                                                                                                                                                                |
| connect          | Semua event masuk lewat satu set listener dan dirutekan memakai `sessionId`: data → terminal tab itu, status/error/host-key → descriptor tab itu.                                                                                                                                                                                                                                                                                               |
| switch           | `activeSessionId` berubah. Tab lama tidak diputus, error dan scrollback-nya tidak dihapus; tab baru di-fit, difokuskan, dan ukurannya dikirim ulang bila sudah connected.                                                                                                                                                                                                                                                                       |
| disconnect       | Hanya tab aktif. Tab ditandai `disconnecting` lebih dulu: input/resize tab itu tidak lagi dikirim dan tombol Disconnect mati, tetapi tab tetap ada dan `hasLiveSessions` tetap true. Setelah `disconnect_ssh` selesai, status menjadi `disconnected` dan scrollback tetap. Bila IPC gagal, `disconnecting` dibatalkan, tab tetap hidup, dan error ditampilkan agar bisa dicoba lagi.                                                            |
| close            | Tab ditandai `closing` (klik kedua diabaikan). Start yang masih berjalan dibatalkan dan ditunggu (`await start.done`) sebelum `disconnect_ssh`, jadi backend tidak pernah mendaftarkan sesi setelah tabnya hilang. Bila stop gagal, tanda `closing` diambil kembali, tab tetap ada, dan error ditampilkan. Bila berhasil, hanya tab itu yang dihapus bersama terminalnya. Tab aktif berikutnya: kanan pada posisi sama, lalu kiri, lalu `null`. |
| window torn down | Seluruh start yang masih pending dibatalkan lebih dulu (sehingga tidak ada sesi baru yang lahir setelah teardown dimulai), lalu best effort `disconnect_ssh` untuk seluruh sesi yang masih hidup, lalu seluruh listener dan terminal dibuang.                                                                                                                                                                                                   |

## Tiga race lifecycle yang ditutup

Bagian ini menjelaskan jaminan yang dipegang kode sekarang. Semuanya punya regression test
deterministik (tanpa `sleep` sebagai sinkronisasi) di `src/lib/session-start.test.ts`,
`src/lib/session-lifecycle.test.ts`, `src/lib/session-tabs.test.ts`, dan `src-tauri/src/ssh/mod.rs`.

### 1. Start tab dapat dibatalkan

`session-start.ts` memegang satu start sebagai lifecycle kecil: phase `queued` → `invoking` →
`finished`, sebuah `cancel()`, dan sebuah `done` yang selesai tepat sekali. Start menunggu terminal
tabnya **dan** seluruh listener, dan membatalkan diri seketika — termasuk ketika listener belum
pernah siap — lewat cancellation signal, sehingga `close` tidak pernah tertahan menunggu listener.

Sebelum memanggil backend, start memeriksa tiga hal: tab masih ada, tab belum `closing`, dan start
belum dibatalkan. Jadi tab yang ditutup sebelum IPC tidak pernah memanggil `connect_ssh` atau
`start_saved_ssh`. Bila IPC sudah berjalan, `close` membatalkan, **menunggu `start.done`**, baru
memanggil `disconnect_ssh`, dan baru setelah itu menghapus tab. Error start hanya ditampilkan bila
tabnya masih ada dan start tidak dibatalkan.

Kredensial Quick Connect tidak pernah masuk descriptor tab. Plan yang membawa kredensial diserahkan
ke `beginSessionStart` sebagai nilai milik start itu sendiri — **bukan** sebagai field dari objek
dependency, karena objek itu dicapture oleh callback yang hidup selama control-nya hidup dan akan
memegang kredensial selama itu juga. Start menyimpannya di satu binding saja, dan binding itu diisi
`null` di `finally`: sukses, gagal, maupun dibatalkan. Callback penutupnya hanya menerima `sessionId`,
jadi ia tidak perlu — dan tidak bisa — ikut menangkap plan; keputusan “ini Quick Connect atau bukan”
dihitung lebih dulu sebagai boolean. Form dibebaskan tepat sekali oleh start yang memang memiliki plan
itu, sehingga submit baru tidak pernah kehilangan kredensialnya. Yang dijamin di sini adalah pelepasan
referensi secara sengaja di level aplikasi, bukan zeroization memori: JavaScript tidak bisa menjamin
byte-nya benar-benar hilang.

### 2. Listener fail closed

Keempat listener (`ssh://data`, `ssh://status`, `ssh://error`, `ssh://host-key`) dipasang satu per satu
dan hanya dianggap siap bila semuanya berhasil. Bila salah satu gagal, listener yang sudah terpasang
dilepas kembali, `listenersReady` **reject** (tidak pernah berubah menjadi fulfilled), `eventsError`
ditampilkan, dan tidak ada sesi yang dimulai — termasuk untuk tab yang menunggu. Promise yang
reject itu tetap reject, jadi setiap start menerima kegagalannya; handler terpisah hanya untuk
menampilkan pesan, bukan untuk menelan error tersebut.

### 3. Backend: sesi yang sedang stopping

`SessionTable` membedakan tiga hal: sesi aktif (`sessions`), id yang masih berhenti (`stopping`), dan
maintenance/restore aktif (`maintaining`).

- Saat stop dimulai, id dipindahkan secara atomik dari `sessions` ke `stopping`.
- `begin_maintenance()` menolak bila ada sesi aktif, ada id yang stopping, atau maintenance lain
  sedang aktif. Jadi restore ditolak selama task SSH masih graceful shutdown atau masih menunggu
  abort — bukan hanya selama masih ada di map.
- Registrasi menolak id yang ada di `sessions` **maupun** `stopping`, sehingga id tidak bisa dipakai
  ulang sampai task lama benar-benar selesai.
- Penanda `stopping` dilepas lewat RAII (`Drop`), jadi dilepas pada semua jalur: graceful, timeout,
  abort, maupun panic. Setelah `abort()`, `JoinHandle` ditunggu benar-benar selesai sebelum penanda
  dilepas, dan mutex tidak pernah dipegang selama menunggu task.

Race register–spawn–attach dihilangkan dengan mendaftarkan session dan task-nya dalam **satu** operasi
yang memegang lock singkat (`launch`): entri dan handle task masuk bersama, jadi `stop` selalu
menemukan keduanya atau tidak menemukan apa pun. Tidak ada task SSH yang bisa berjalan tanpa entri,
dan tidak ada entri tanpa task. Setiap registrasi mendapat nomor generasi, dan cleanup task hanya
boleh menghapus entri dengan generasi yang sama, sehingga task yang selesai terlambat tidak bisa
mengambil alih tempat sesi yang lebih baru. Fungsi `attach_task` sudah tidak ada.

## Checklist pengujian manual (native Tauri)

Legenda:

- `- [ ]` Belum diuji
- `- [x]` Lulus

Status saat ini: **seluruh item masih belum diuji.** Belum ada manusia yang melewati daftar ini
setelah perubahan terakhir Phase 1F.

Menyiapkan:

1. `npm install`, lalu jalankan aplikasi dengan `npm run tauri dev`.
2. Siapkan dua host untuk diuji, minimal satu di antaranya saved Host dengan Credential. Server
   sekali pakai dari image integration test bisa dipakai; lihat `tests/integration/ssh/README.md`,
   bagian _Running the server by hand_.
3. Untuk bagian host-key prompt, siapkan host yang key-nya belum pernah disetujui (atau lupakan
   known host-nya lebih dulu dari section Known hosts).

- [ ] Buka dua saved host secara bersamaan; keduanya muncul sebagai tab terpisah dan keduanya
      mencapai shell.
- [ ] Buka satu saved host dan satu Quick Connect; form Quick Connect tetap dapat dipakai setelah
      tab saved Host terbuka.
- [ ] Jalankan command berbeda pada setiap tab; output setiap command hanya muncul di tabnya.
- [ ] Berpindah tab beberapa kali; scrollback tiap tab tetap utuh, tidak ada tab yang terputus, dan
      error atau status tab lain tidak berubah.
- [ ] Resize window pada masing-masing tab; terminal di tab itu mengikuti ukuran window dan tab lain
      tidak ikut berubah (buktikan dengan `stty size` atau `tput cols` di tiap tab).
- [ ] Buka dua host-key prompt (dua host yang belum dipercaya) berdekatan; jawab keduanya dan
      pastikan tidak ada jawaban yang tertukar, serta prompt yang belum dijawab tidak hilang.
- [ ] Jalankan Snippet pada tab aktif; dialog preview menyebut tab yang benar dan command muncul di
      tab itu saja.
- [ ] Buka preview Snippet, pindah tab, lalu coba konfirmasi: tombol Run tidak aktif dan targetnya
      tetap tab awal; kembali ke tab itu dan barulah command terkirim.
- [ ] Disconnect satu tab; tab lain tetap hidup dan masih bisa menjalankan command, sedangkan tab
      yang diputus tetap menampilkan scrollback-nya dan tidak menerima input.
- [ ] Close tab yang sedang connected; hanya sesi itu yang berhenti (periksa proses di sisi server
      atau daftar login), tab lain tidak terpengaruh.
- [ ] Close tab aktif; tab berikutnya yang aktif adalah tab di kanan pada posisi yang sama, atau tab
      di kiri bila tab yang ditutup paling kanan, dan bila itu tab terakhir tidak ada tab aktif.
- [ ] Coba restore backup saat beberapa sesi masih aktif; restore ditolak, dan aplikasi tidak memutus
      sesi mana pun sendiri.
- [ ] Tutup tab sesaat setelah menekan Connect (sebelum shell muncul); tidak ada sesi yang lahir di
      sisi server untuk tab itu, dan tab lain tidak terpengaruh.
- [ ] Tutup tab yang sedang connecting ke saved Host yang lambat (vault terkunci atau resolusi
      lambat); setelah host akhirnya menjawab, tidak ada tab baru yang muncul dan tidak ada sesi
      backend yang tertinggal.
- [ ] Tekan Disconnect pada satu tab; tombolnya mati selama proses berhenti, tab tetap menampilkan
      scrollback dan tidak menerima input, dan tombol Restore backup tetap mati sampai statusnya
      benar-benar `disconnected`.
- [ ] Tutup window saat ada tab yang masih connecting; tidak ada proses yang tertinggal dan tidak ada
      sesi yang menggantung di sisi server.
