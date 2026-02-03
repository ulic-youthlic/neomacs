{ lib
, stdenv
, rust-neomacs
, rust-cbindgen
, pkg-config
, autoconf
, automake
, texinfo
, llvmPackages
, ncurses
, gnutls
, zlib
, libxml2
, fontconfig
, freetype
, harfbuzz
, cairo
, gtk4
, glib
, graphene
, pango
, gdk-pixbuf
, gst_all_1
, libsoup_3
, glib-networking
, libjpeg
, libtiff
, giflib
, libpng
, librsvg
, libwebp
, dbus
, sqlite
, libselinux
, tree-sitter
, gmp
, libgccjit
, libGL
, libxkbcommon
, mesa
, libdrm
, libgbm
, wayland
, wpewebkit
, libwpe
, libwpe-fdo
, weston
}:

stdenv.mkDerivation rec {
  pname = "neomacs";
  version = "30.0.50-neomacs";

  src = ./..;

  nativeBuildInputs = [
    rust-neomacs
    rust-cbindgen
    pkg-config
    autoconf
    automake
    texinfo
    llvmPackages.libclang
  ];

  buildInputs = [
    ncurses
    gnutls
    zlib
    libxml2
    fontconfig
    freetype
    harfbuzz
    cairo
    gtk4
    glib
    graphene
    pango
    gdk-pixbuf
    gst_all_1.gstreamer
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good
    gst_all_1.gst-plugins-bad
    gst_all_1.gst-plugins-ugly
    gst_all_1.gst-libav
    gst_all_1.gst-plugins-rs
    libsoup_3
    glib-networking
    libjpeg
    libtiff
    giflib
    libpng
    librsvg
    libwebp
    dbus
    sqlite
    libselinux
    tree-sitter
    gmp
    libgccjit
    libGL
    libxkbcommon
    mesa
    libdrm
    libgbm
    wayland
    wpewebkit
    libwpe
    libwpe-fdo
    weston
  ];

  LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";

  # Build Rust library first, then Emacs
  preConfigure = ''
    echo "Building neomacs-display Rust library..."
    pushd rust/neomacs-display
    cargo build --release
    popd

    echo "Running autogen.sh..."
    ./autogen.sh
  '';

  configureFlags = [
    "--with-neomacs"
    "--with-native-compilation"
    "--with-gnutls"
    "--with-xml2"
    "--with-tree-sitter"
    "--with-modules"
  ];

  # Set up environment for WPE WebKit
  preBuild = ''
    export WPE_BACKEND_LIBRARY="${libwpe-fdo}/lib/libWPEBackend-fdo-1.0.so"
    export GIO_MODULE_DIR="${glib-networking}/lib/gio/modules"
  '';

  # Wrap the binary with required environment variables
  postInstall = ''
    wrapProgram $out/bin/emacs \
      --set WPE_BACKEND_LIBRARY "${libwpe-fdo}/lib/libWPEBackend-fdo-1.0.so" \
      --set GIO_MODULE_DIR "${glib-networking}/lib/gio/modules" \
      --set WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS "1" \
      --prefix PATH : "${wpewebkit}/libexec/wpe-webkit-2.0"
  '';

  meta = with lib; {
    description = "Neomacs - GPU-accelerated Emacs with GTK4, GStreamer, and WPE WebKit";
    homepage = "https://github.com/eval-exec/neomacs";
    license = licenses.gpl3Plus;
    platforms = platforms.linux;
    maintainers = [ ];
  };
}
