using Avalonia;
using System;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using CsBindgen;

namespace cs_gui;

internal static class Program {
    // Initialization code. Don't use any Avalonia, third-party APIs or any
    // SynchronizationContext-reliant code before AppMain is called: things aren't initialized
    // yet and stuff might break.
    [STAThread]
    public static void Main(string[] args) {
        unsafe {
            var path = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);

            fixed (char* ptr = path) {
                NativeMethods.init((ushort*)ptr, (nuint)path.Length);
            }
        }

        BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
    }

    // Avalonia configuration, don't remove; also used by visual designer.
    private static AppBuilder BuildAvaloniaApp() {
        return AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
    }

    public static ReadOnlySpan<byte> CopyRefString(RefStringWrapper wrapper) {
        unsafe {
            return new ReadOnlySpan<byte>(wrapper.char_ptr, (int)wrapper.len);
        }
    }

    public static string CopyAndFreeOwnedString(OwnedStringWrapper wrapper) {
        unsafe {
            var str = Encoding.UTF8.GetString(wrapper.char_ptr, (int)wrapper.len);
            NativeMethods.free_owned_string_wrapper(wrapper);
            return str;
        }
    }
}

internal static class SafeNativeMethods {
    public static Task GetManifest() =>
        Task.Run(() => {
            unsafe {
                var taskPointer = NativeMethods.get_version_manifest();
                while (!NativeMethods.poll_manifest_task(taskPointer)) { }

                var value = NativeMethods.await_version_manifest(taskPointer);

                if (value.code != Code.Success) {
                    throw new RustException(value);
                }
            }
        });

    public static bool IsManifestNull() => NativeMethods.is_manifest_null();

    public static ReadOnlySpan<byte> GetLatestRelease() => Program.CopyRefString(NativeMethods.get_latest_release());

    public static nuint ManifestLength() => NativeMethods.get_manifest_len();

    public static ReadOnlySpan<byte> GetVersionId(nuint index) => Program.CopyRefString(NativeMethods.get_name(index));

    public static ReleaseType GetVersionType(nuint index) => NativeMethods.get_type(index);

    public static Task GetVersion(nuint index, CancellationToken token) =>
        Task.Run(() => {
            unsafe {
                token.ThrowIfCancellationRequested();
                var versionTaskPointer = NativeMethods.get_version_task(index);
                while (!NativeMethods.poll_version_task(versionTaskPointer)) {
                    if (!token.IsCancellationRequested) continue;
                    NativeMethods.cancel_version_task(versionTaskPointer);
                    token.ThrowIfCancellationRequested();
                }

                var value = NativeMethods.await_version_task(versionTaskPointer);
                if (value.code != Code.Success) throw new RustException(value);

                if (token.IsCancellationRequested) return;

                var assetIndexTaskPointer = NativeMethods.get_asset_index();
                while (!NativeMethods.poll_asset_index(assetIndexTaskPointer)) {
                    if (!token.IsCancellationRequested) continue;
                    NativeMethods.cancel_asset_index(assetIndexTaskPointer);
                    token.ThrowIfCancellationRequested();
                }

                var v = NativeMethods.await_asset_index(assetIndexTaskPointer);
                if (v.code != Code.Success) throw new RustException(v);
            }
        }, token);
    
    public static Task Auth() =>
        Task.Run(() => {
            unsafe {
                var taskPointer = NativeMethods.start_auth_loop();
                while (!NativeMethods.poll_auth_loop(taskPointer)) {
                    Task.Delay(100); 
                }

                var value = NativeMethods.await_auth_loop(taskPointer);

                if (value.code != Code.Success) {
                    throw new RustException(value);
                }
            }
        });

    public static Task GetDeviceResponse => Task.Run(() => {
        unsafe {
            var responseTask = NativeMethods.get_device_response();
            while (!NativeMethods.poll_device_response(responseTask)) { }

            var response = NativeMethods.await_device_response(responseTask);
            if (response.code != Code.Success) throw new RustException(response);
        }
    });

    public static string GetCode() {
        return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.get_user_code()));
    }
    
    public static string GetUrl() {
        return Encoding.UTF8.GetString(Program.CopyRefString(NativeMethods.get_url()));
    }
}

internal class RustException(NativeReturn value)
    : Exception(value.code + " " + Program.CopyAndFreeOwnedString(value.error));
