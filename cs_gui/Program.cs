using Avalonia;
using System;
using System.Text;
using System.Threading.Tasks;
using CsBindgen;

namespace cs_gui;

class Program
{
    // Initialization code. Don't use any Avalonia, third-party APIs or any
    // SynchronizationContext-reliant code before AppMain is called: things aren't initialized
    // yet and stuff might break.
    [STAThread]
    public static void Main(string[] args)
    {
        unsafe
        {
            var path = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);

            fixed (char* ptr = path)
            {
                NativeMethods.init((ushort*) ptr, (nuint) path.Length);
            }
        }
        
        BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
    }

    // Avalonia configuration, don't remove; also used by visual designer.
    public static AppBuilder BuildAvaloniaApp()
    {
        return AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
    }
    
    public static ReadOnlySpan<byte> CopyRefString(RefStringWrapper wrapper)
    {
        unsafe
        {
            return new ReadOnlySpan<byte>(wrapper.char_ptr, (int) wrapper.len);
        }
    }

    public static string CopyAndFreeOwnedString(OwnedStringWrapper wrapper)
    {
        unsafe
        {
            var str = Encoding.UTF8.GetString(wrapper.char_ptr, (int) wrapper.len);
            NativeMethods.free_string_wrapper(wrapper);
            return str;
        }
    }
}

internal static class SafeNativeMethods
{
    public static Task GetManifest()
    {
        return Task.Run(() =>
        {
            unsafe
            {
                var taskPointer = NativeMethods.get_version_manifest();
                while (!NativeMethods.poll_manifest_task(taskPointer))
                {
                    
                }

                var value = NativeMethods.get_manifest(taskPointer);

                if (value.code != Code.Success)
                {
                    throw new RustException(value);
                }
            }
        });
    }

    public static bool IsManifestNull()
    {
        return NativeMethods.is_manifest_null();
    }

    public static ReadOnlySpan<byte> GetLatestRelease()
    {
        var rawString = NativeMethods.get_latest_release();
        return Program.CopyRefString(rawString);
    }

    public static nuint ManifestLength()
    {
        return NativeMethods.get_manifest_len();
    }

    public static ReadOnlySpan<byte> GetVersionId(nuint index)
    {
        var rawString = NativeMethods.get_name(index);
        return Program.CopyRefString(rawString);
    }

    public static ReleaseType GetVersionType(nuint index)
    {
        return NativeMethods.get_type(index);
    }
}

internal class RustException(NativeReturn value) : Exception(value.code + " " + Program.CopyAndFreeOwnedString(value.error));
