using System;
using System.Collections.Generic;
using System.Text;
using Avalonia.Controls;

namespace cs_gui;

public partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
        
        var task = SafeNativeMethods.GetManifest();

        try
        {
            task.Wait();
        }
        catch (RustException e)
        {
            Console.WriteLine(e);
        }

        var list = new List<string>();
        var len = SafeNativeMethods.ManifestLength();
        
        for (UIntPtr idx = 0; idx < len; idx++)
        {
            list.Add(Encoding.UTF8.GetString(SafeNativeMethods.GetVersionId(idx)));
        }

        VersionSelectBox.ItemsSource = list;
    }
}

public readonly struct LibraryMemory {
    private readonly unsafe char* _ptr;
    private readonly int _len;

    internal unsafe LibraryMemory(char* ptr, int len) {
        _ptr = ptr;
        _len = len;
    }

    public unsafe Span<char> Span => new (_ptr, _len);
}