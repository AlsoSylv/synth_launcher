using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Interactivity;

namespace cs_gui;

public partial class MainWindow : Window {
    private Task? _versionTask;
    private readonly CancellationTokenSource _token = new();
    private static UserCodeWindow? _userCodeWindow;

    public MainWindow() {
        InitializeComponent();

        var task = SafeNativeMethods.GetManifest();

        try {
            task.Wait();

            var list = new List<string>();
            var len = SafeNativeMethods.ManifestLength();

            for (UIntPtr idx = 0; idx < len; idx++) {
                list.Add(Encoding.UTF8.GetString(SafeNativeMethods.GetVersionId(idx)));
            }

            VersionSelectBox.ItemsSource = list;
        }
        catch (AggregateException ae) {
            ae.Handle(x => {
                if (x is not RustException) return false;
                Console.WriteLine(x);
                return true;
            });
        }
    }

    public static void CloseUserCodeWindow() {
        Debug.Assert(_userCodeWindow != null, nameof(_userCodeWindow) + " != null");
        _userCodeWindow.Close();
    }

    private async void VersionSelectBox_OnSelectionChanged(object? _, SelectionChangedEventArgs e) {
        if (_versionTask == null) {
            var index = VersionSelectBox.SelectedIndex;
            _versionTask = SafeNativeMethods.GetVersion((nuint)index, _token.Token);
        }
        else {
            await _token.CancelAsync();
            _token.TryReset();

            var index = VersionSelectBox.SelectedIndex;
            _versionTask = SafeNativeMethods.GetVersion((nuint)index, _token.Token);
        }
    }

    private async void Button_OnClick(object? sender, RoutedEventArgs e) {
        await SafeNativeMethods.GetDeviceResponse;
        var window = new UserCodeWindow(SafeNativeMethods.GetCode(), SafeNativeMethods.GetUrl());
        
        window.Show();

        _userCodeWindow = window;
    }
}