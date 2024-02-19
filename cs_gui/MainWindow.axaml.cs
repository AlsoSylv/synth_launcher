using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Threading;
using CsBindgen;

namespace cs_gui;

public partial class MainWindow : Window
{
    private Task? _versionTask;
    private readonly CancellationTokenSource _token = new();

    public MainWindow()
    {
        InitializeComponent();

        var task = SafeNativeMethods.GetManifest();

        VersionSelectBox.IsEnabled = false;
        LibraryProgressBar.Minimum = 0;
        LibraryProgressBar.Maximum = 1;
        LibraryProgressBar.ShowProgressText = true;

        Dispatcher.UIThread.InvokeAsync(async () =>
        {
            try
            {
                await task;
                VersionSelectBox.IsEnabled = true;
                var list = new List<string>();
                var len = SafeNativeMethods.ManifestLength();

                for (UIntPtr idx = 0; idx < len; idx++)
                    list.Add(Encoding.UTF8.GetString(SafeNativeMethods.GetVersionId(idx)));

                VersionSelectBox.ItemsSource = list;
            }
            catch (AggregateException ae)
            {
                ae.Handle(x =>
                {
                    if (x is not RustException) return false;
                    Console.WriteLine(x);
                    return true;
                });
            }
        });
    }
    
    private void VersionSelectBox_OnSelectionChanged(object? _, SelectionChangedEventArgs e)
    {
        if (_versionTask == null)
        {
            var index = VersionSelectBox.SelectedIndex;
            _versionTask = SafeNativeMethods.GetVersion((nuint)index, _token.Token);
        }
        else
        {
            if (!_versionTask.IsCompleted) _token.Cancel();
            _token.TryReset();

            var index = VersionSelectBox.SelectedIndex;
            _versionTask = SafeNativeMethods.GetVersion((nuint)index, _token.Token);
        }
    }

    private void Button_OnClick(object? sender, RoutedEventArgs e)
    {
        LoginButton.IsEnabled = false;

        Dispatcher.UIThread.InvokeAsync(async () =>
        {
            await SafeNativeMethods.GetDeviceResponse;
            var window = new UserCodeWindow(SafeNativeMethods.GetCode(), SafeNativeMethods.GetUrl());

            window.Show();

            window.Closed += delegate { LoginButton.IsEnabled = true; };
        });
    }

    private void PlayButton_OnClick(object? sender, RoutedEventArgs e) {
        Dispatcher.UIThread.InvokeAsync(async () => {
            ulong total = 0;
            ulong finished = 0;
            await _versionTask!;
            Task assetTask;
            unsafe {
                assetTask = SafeNativeMethods.GetAssets(&total, &finished);
            }
            while (!assetTask.IsCompleted) {
                if (total != 0) {
                    LibraryProgressBar.Value = finished / (double) total;
                }
                await Task.Delay(100);
            }
            LibraryProgressBar.Value = 1;
        });
    }
}