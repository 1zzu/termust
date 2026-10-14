# Checklist pengujian manual Phase 0

Dokumen ini mencatat hal-hal yang hanya bisa dibuktikan oleh manusia di depan aplikasi
yang benar-benar berjalan. Unit test dan integration test **tidak** menggantikan item di
sini, dan tidak ada item yang boleh ditandai lulus hanya karena test hijau.

Legenda:

- `- [ ]` Belum diuji
- `- [x]` Lulus

Status saat ini: **seluruh item masih belum diuji.** Belum ada manusia yang melewati
daftar ini setelah perubahan terakhir pada Phase 0.

## Menyiapkan

1. Jalankan aplikasi dari akar repositori:

   ```sh
   npm install
   npm run tauri dev
   ```

   Output terminal ini juga yang diperiksa pada bagian E.

2. Siapkan server SSH untuk diuji. Pilih salah satu:
   - server milik sendiri yang boleh dipakai untuk uji coba, atau
   - server sekali pakai dari image integration test; lihat
     `tests/integration/ssh/README.md`, bagian _Running the server by hand_
     (`127.0.0.1:2222`, user `tester`, password dicetak oleh perintah di sana).

3. Untuk bagian C, siapkan dua hal: server yang host key-nya bisa diubah (cukup dengan
   me-restart container-nya) dan aplikasi yang tetap berjalan selama pengujian.

## A. Aplikasi dan form

- [ ] Aplikasi Tauri terbuka, menampilkan form Connection dan terminal kosong di bawahnya.
- [ ] Form dapat diisi: host, port (default 22), username, dan pilihan autentikasi. Field
      kredensial berubah sesuai metode yang dipilih (password vs private key + passphrase).
- [ ] Selama proses connect, field dinonaktifkan, tombol berubah menjadi "Connecting…", dan
      tombol Disconnect baru aktif setelah ada session.

## B. Autentikasi

- [ ] Password authentication membuka shell dan menampilkan prompt server.
- [ ] Private-key authentication dengan key Ed25519 membuka shell.
- [ ] Private-key authentication dengan key RSA bertanda tangan SHA-2 membuka shell.
- [ ] Key yang dienkripsi passphrase dapat dibuka dengan passphrase yang benar.
- [ ] Password salah hanya menghasilkan pesan error yang bisa ditindaklanjuti; pesan itu
      tidak memuat password yang diketik (periksa juga output `npm run tauri dev`).
- [ ] Setelah session berakhir (disconnect atau error), field password, private key, dan
      passphrase kembali kosong di form.

## C. Verifikasi host key

- [ ] Koneksi pertama ke host baru memunculkan dialog fingerprint berisi host, port,
      algoritma, dan fingerprint `SHA256:...`.
- [ ] Tombol Cancel membatalkan koneksi: tidak ada shell, tidak ada output terminal, dan
      status berakhir sebagai error.
- [ ] Tombol "Accept and connect" melanjutkan koneksi sampai shell terbuka.
- [ ] Setelah aplikasi di-restart, koneksi ke host yang sama meminta fingerprint kembali
      karena known-hosts hanya hidup selama aplikasi berjalan.
- [ ] Host key yang berubah diblokir: hubungkan sekali dan setujui, restart server uji
      sehingga host key-nya baru, lalu connect lagi dari aplikasi yang masih berjalan. Kali
      ini koneksi ditolak tanpa dialog persetujuan, dan pesannya menyebut host key berubah.
- [ ] Server yang hanya menawarkan `ssh-rsa` (SHA-1) ditolak dengan pesan yang menyarankan
      key Ed25519 atau ECDSA, bukan dijatuhkan ke SHA-1.

## D. Terminal

- [ ] Ketikan biasa (huruf, angka, simbol) masuk ke server dan tampil di terminal.
- [ ] Ctrl+C menghentikan proses yang berjalan di server.
- [ ] Ctrl+D mengirim EOF dan menutup shell.
- [ ] Esc dan Alt tidak ditelan aplikasi: Esc membatalkan mode editor atau menyelesaikan
      urutan escape, dan Alt+huruf sampai ke program di server.
- [ ] Tab menyelesaikan perintah (completion) di shell.
- [ ] Arrow kiri/kanan menggerakkan kursor, arrow atas/bawah menelusuri history.
- [ ] Paste (Cmd+V di macOS, Ctrl+Shift+V di Linux/Windows) menempelkan teks ke terminal.
- [ ] Resize window mengubah ukuran terminal dan server menerimanya, mis. `stty size`
      menunjukkan kolom dan baris yang baru.
- [ ] Perintah dengan banyak output (mis. `ls -R /` atau `cat` file besar) tetap terbaca dan
      bisa di-scroll tanpa merusak tampilan.

## E. Siklus session

- [ ] Disconnect menghentikan session: prompt hilang, tombol Connect aktif kembali, dan
      tombol Disconnect tidak aktif lagi.
- [ ] Setelah disconnect, ketikan tidak lagi dikirim ke server.
- [ ] Connect ulang tidak menerima output session sebelumnya: ketik perintah di session
      baru dan pastikan tidak ada keluaran dari perintah yang dijalankan di session lama
      yang muncul setelah reconnect.
- [ ] Menutup window aplikasi saat session berjalan tidak meninggalkan proses yang
      menggantung (periksa `ps` untuk proses aplikasi setelah window ditutup).

## F. Kebersihan data

- [ ] Tidak ada password, private key, atau passphrase di log: periksa seluruh output
      `npm run tauri dev` dan console webview (klik kanan → Inspect Element) selama
      pengujian.
- [ ] Tidak ada file kredensial yang dibuat di disk selama pengujian, mis. tidak ada
      direktori baru di `~/Library/Application Support/com.termust.desktop` (macOS) atau
      `~/.local/share/com.termust.desktop` (Linux).
- [ ] Private key yang ditempel hanya dibaca; isinya tidak ditulis kembali ke file mana pun.
