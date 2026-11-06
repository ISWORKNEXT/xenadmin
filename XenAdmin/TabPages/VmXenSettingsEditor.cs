/* Copyright (c) ISWORKNEXT
 *
 * Licensed under the BSD 2-Clause license, the same terms as the rest of
 * this repository.
 */

using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Drawing;
using System.Windows.Forms;
using XenAdmin.Actions;
using XenAPI;

namespace XenAdmin.TabPages
{
    public enum VmXenSettingsArea
    {
        General,
        Cpu,
        SuspendSr
    }

    /// <summary>
    /// Edits a slice of the XO "Xen settings" directly on a main-window tab.
    /// </summary>
    public class VmXenSettingsEditor : UserControl
    {
        private readonly VmXenSettingsArea _area;
        private readonly TableLayoutPanel _table;
        private readonly Button _apply;
        private Label _hint;
        private Label _error;
        private readonly Size _openMinimum;

        private VM _vm;
        private bool _loading;
        private bool _dirty;

        private Label _mode;
        private TextBox _startDelay;
        private CheckBox _autoPower;
        private CheckBox _protectDelete;
        private CheckBox _protectShutdown;
        private CheckBox _preventMigrate;
        private CheckBox _vendorDevice;
        private CheckBox _nested;
        private CheckBox _viridian;
        private ComboBox _firmware;
        private CheckBox _vga;
        private TextBox _videoRam;
        private CheckBox _secureBoot;
        private Label _secureStatus;
        private TextBox _weight;
        private TextBox _cap;
        private TextBox _mask;
        private ComboBox _suspendSr;

        public VmXenSettingsEditor(VmXenSettingsArea area)
        {
            _area = area;
            switch (area)
            {
                case VmXenSettingsArea.General:
                    _openMinimum = new Size(320, 520);
                    break;
                case VmXenSettingsArea.Cpu:
                    _openMinimum = new Size(320, 148);
                    break;
                case VmXenSettingsArea.SuspendSr:
                    _openMinimum = new Size(320, 96);
                    break;
                default:
                    throw new InvalidOperationException(area.ToString());
            }

            BackColor = SystemColors.Control;
            Padding = new Padding(8, 6, 8, 6);
            MinimumSize = _openMinimum;

            _table = new TableLayoutPanel
            {
                Dock = DockStyle.Top,
                ColumnCount = 2,
                AutoSize = true,
                AutoSizeMode = AutoSizeMode.GrowAndShrink
            };
            _table.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 220));
            _table.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
            Controls.Add(_table);

            switch (area)
            {
                case VmXenSettingsArea.General:
                    BuildGeneral();
                    break;
                case VmXenSettingsArea.Cpu:
                    BuildCpu();
                    break;
                case VmXenSettingsArea.SuspendSr:
                    BuildSuspend();
                    break;
                default:
                    throw new InvalidOperationException(area.ToString());
            }

            _error = new Label
            {
                AutoSize = true,
                ForeColor = Color.Firebrick,
                Visible = false,
                Margin = new Padding(0, 4, 0, 0)
            };
            AddFull(_error);

            _apply = new Button
            {
                Text = "Apply",
                AutoSize = true,
                Enabled = false,
                Margin = new Padding(0, 6, 0, 0)
            };
            _apply.Click += ApplyClick;
            AddFull(_apply);
        }

        public void Bind(VM vm)
        {
            if (ReferenceEquals(vm, _vm))
            {
                if (!_dirty)
                    LoadFromVm();
                UpdateChrome();
                return;
            }

            Unsubscribe();
            _vm = vm;
            _dirty = false;
            Subscribe();
            LoadFromVm();
            UpdateChrome();
        }

        public bool HasEdits => _dirty;

        public string Summary => _mode == null ? "" : _mode.Text;

        public bool TryValidate(out string error)
        {
            return Valid(out error);
        }

        public void ShowValidation()
        {
            string error;
            Valid(out error);
            _error.Text = error ?? "";
            _error.Visible = !string.IsNullOrEmpty(error);
        }

        public void HideValidation()
        {
            _error.Visible = false;
            _error.Text = "";
        }

        public void ApplyTo(VM target)
        {
            if (target == null)
                return;

            var snap = Read();
            switch (snap.Area)
            {
                case VmXenSettingsArea.General:
                    ApplyGeneral(target, target, snap);
                    break;
                case VmXenSettingsArea.Cpu:
                    target.VCPUs_params = VM.SetDictionaryKey(target.VCPUs_params, "weight", snap.Weight);
                    target.VCPUs_params = VM.SetDictionaryKey(target.VCPUs_params, "cap", snap.Cap);
                    target.VCPUs_params = VM.SetDictionaryKey(target.VCPUs_params, "mask",
                        string.IsNullOrEmpty(snap.Mask) ? null : snap.Mask);
                    break;
                case VmXenSettingsArea.SuspendSr:
                    target.suspend_SR = new XenRef<SR>(snap.SuspendSr == null ? Helper.NullOpaqueRef : snap.SuspendSr.opaque_ref);
                    break;
                default:
                    throw new InvalidOperationException(snap.Area.ToString());
            }
        }

        public void UseInProperties()
        {
            _apply.Visible = false;
            AutoSize = false;
            Dock = DockStyle.Top;
        }

        private void Subscribe()
        {
            if (_vm != null)
                _vm.PropertyChanged += VmPropertyChanged;
        }

        private void Unsubscribe()
        {
            if (_vm != null)
                _vm.PropertyChanged -= VmPropertyChanged;
        }

        private void VmPropertyChanged(object sender, PropertyChangedEventArgs e)
        {
            if (_dirty)
                return;
            if (IsHandleCreated)
                Program.Invoke(this, LoadFromVm);
        }

        private void UpdateChrome()
        {
            var show = _vm != null;
            Visible = show;
            if (!show)
            {
                AutoSize = false;
                MinimumSize = Size.Empty;
                Height = 0;
                return;
            }

            AutoSize = true;
            AutoSizeMode = AutoSizeMode.GrowAndShrink;
            MinimumSize = _openMinimum;
        }

        private void BuildGeneral()
        {
            AddTitle("Xen settings", "Firmware and platform changes apply the next time the VM starts.");
            _mode = new Label { AutoSize = true, Anchor = AnchorStyles.Left };
            AddPair("Virtualization mode", _mode);
            _startDelay = AddText("Start delay (seconds)");
            _weight = AddText("CPU weight");
            _cap = AddText("CPU cap");
            _mask = AddText("CPU mask");
            _autoPower = AddCheck("Auto power on");
            _protectDelete = AddCheck("Protect from accidental deletion");
            _protectShutdown = AddCheck("Protect from accidental shutdown");
            _preventMigrate = AddCheck("Prevent migration");
            _vendorDevice = AddCheck("Manage Citrix PV drivers via Windows Update");
            _nested = AddCheck("Nested virtualization");
            _viridian = AddCheck("Viridian");
            _firmware = AddCombo("Boot firmware", "bios", "uefi");
            _vga = AddCheck("VGA");
            _videoRam = AddText("Video RAM (MiB)");
            _secureBoot = AddCheck("Secure boot");
            _secureStatus = new Label { AutoSize = true, Anchor = AnchorStyles.Left };
            AddPair("Secure boot status", _secureStatus);
        }

        private void BuildCpu()
        {
            AddTitle("CPU scheduling", "Weight, cap and mask are saved on this page. Cap 0 means no cap.");
            _weight = AddText("CPU weight");
            _cap = AddText("CPU cap");
            _mask = AddText("CPU mask");
        }

        private void BuildSuspend()
        {
            AddTitle("Suspend storage", "Storage repository used when this VM is suspended.");
            _suspendSr = AddCombo("Suspend SR");
        }

        private void AddTitle(string title, string hint)
        {
            AddFull(new Label
            {
                Text = title,
                AutoSize = true,
                Font = Program.DefaultFontBold ?? new Font(SystemFonts.MessageBoxFont, FontStyle.Bold),
                Margin = new Padding(0, 0, 0, 2)
            });
            _hint = new Label
            {
                Text = hint,
                AutoSize = true,
                MaximumSize = new Size(640, 0),
                Margin = new Padding(0, 0, 0, 6)
            };
            AddFull(_hint);
        }

        private CheckBox AddCheck(string text)
        {
            var box = new CheckBox
            {
                Text = text,
                AutoSize = true,
                Margin = new Padding(0, 2, 0, 2)
            };
            box.CheckedChanged += Edited;
            AddFull(box);
            return box;
        }

        private TextBox AddText(string caption)
        {
            var box = new TextBox { Anchor = AnchorStyles.Left | AnchorStyles.Right };
            box.TextChanged += Edited;
            AddPair(caption, box);
            return box;
        }

        private ComboBox AddCombo(string caption, params string[] items)
        {
            var box = new ComboBox
            {
                DropDownStyle = ComboBoxStyle.DropDownList,
                Anchor = AnchorStyles.Left | AnchorStyles.Right
            };
            foreach (var item in items)
                box.Items.Add(item);
            box.SelectedIndexChanged += Edited;
            AddPair(caption, box);
            return box;
        }

        private int _row;

        private void AddFull(Control control)
        {
            EnsureRow();
            _table.Controls.Add(control, 0, _row);
            _table.SetColumnSpan(control, 2);
            _row++;
        }

        private void AddPair(string caption, Control editor)
        {
            EnsureRow();
            _table.Controls.Add(new Label
            {
                Text = caption,
                AutoSize = true,
                Anchor = AnchorStyles.Left,
                Margin = new Padding(0, 6, 8, 0)
            }, 0, _row);
            _table.Controls.Add(editor, 1, _row);
            _row++;
        }

        private void EnsureRow()
        {
            _table.RowCount = _row + 1;
            _table.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        }

        private void Edited(object sender, EventArgs e)
        {
            if (_loading)
                return;
            _dirty = true;
            RefreshApply();
        }

        private void LoadFromVm()
        {
            _loading = true;
            try
            {
                _error.Visible = false;
                _error.Text = "";
                if (_vm == null)
                    return;

                switch (_area)
                {
                    case VmXenSettingsArea.General:
                        LoadGeneral();
                        break;
                    case VmXenSettingsArea.Cpu:
                        LoadCpu();
                        break;
                    case VmXenSettingsArea.SuspendSr:
                        LoadSuspend();
                        break;
                    default:
                        throw new InvalidOperationException(_area.ToString());
                }
            }
            finally
            {
                _loading = false;
                RefreshApply();
            }
        }

        private void LoadGeneral()
        {
            _mode.Text = _vm.IsHVM() ? "Hardware virtualization (HVM)" : "Paravirtualized (PV)";
            _startDelay.Text = _vm.start_delay.ToString();
            _weight.Text = _vm.GetVcpuWeight().ToString();
            _cap.Text = Value(_vm.VCPUs_params, "cap") ?? "0";
            _mask.Text = Value(_vm.VCPUs_params, "mask") ?? "";
            _autoPower.Checked = _vm.GetAutoPowerOn();
            _protectDelete.Checked = Blocked(vm_operations.destroy);
            _protectShutdown.Checked = Blocked(vm_operations.clean_shutdown) || Blocked(vm_operations.hard_shutdown);
            _preventMigrate.Checked = Blocked(vm_operations.pool_migrate);
            _vendorDevice.Checked = _vm.has_vendor_device;
            _nested.Checked = Flag("nested-virt") || Flag("exp-nested-hvm");
            _viridian.Checked = Flag("viridian");
            var firmware = Value(_vm.HVM_boot_params, "firmware");
            _firmware.SelectedItem = firmware == "uefi" ? "uefi" : "bios";
            _firmware.Enabled = _vm.IsHVM();
            var vga = Value(_vm.platform, "vga");
            _vga.Checked = !string.IsNullOrEmpty(vga) && vga != "false";
            _videoRam.Text = Value(_vm.platform, "videoram") ?? "";
            _secureBoot.Checked = string.Equals(_vm.GetSecureBootMode(), "true", StringComparison.OrdinalIgnoreCase);
            var status = _vm.GetSecureBootMode();
            _secureStatus.Text = string.IsNullOrEmpty(status) || status == "false"
                ? "Secure boot not enforced"
                : status;
        }

        private void LoadCpu()
        {
            _weight.Text = _vm.GetVcpuWeight().ToString();
            _cap.Text = Value(_vm.VCPUs_params, "cap") ?? "0";
            _mask.Text = Value(_vm.VCPUs_params, "mask") ?? "";
        }

        private void LoadSuspend()
        {
            _suspendSr.Items.Clear();
            _suspendSr.Items.Add(new SrChoice(null));
            if (_vm.Connection != null)
            {
                var srs = new List<SR>(_vm.Connection.Cache.SRs);
                srs.Sort();
                foreach (var sr in srs)
                {
                    if (sr == null || sr.IsToolsSR())
                        continue;
                    if (string.Equals(sr.content_type, "iso", StringComparison.OrdinalIgnoreCase))
                        continue;
                    if (!sr.Show(true))
                        continue;
                    _suspendSr.Items.Add(new SrChoice(sr));
                }
            }

            var selected = 0;
            var current = _vm.suspend_SR == null || Helper.IsNullOrEmptyOpaqueRef(_vm.suspend_SR.opaque_ref)
                ? null
                : _vm.suspend_SR.opaque_ref;
            if (current != null)
            {
                for (var i = 0; i < _suspendSr.Items.Count; i++)
                {
                    if (_suspendSr.Items[i] is SrChoice choice && choice.Sr != null && choice.Sr.opaque_ref == current)
                    {
                        selected = i;
                        break;
                    }
                }
            }
            _suspendSr.SelectedIndex = _suspendSr.Items.Count == 0 ? -1 : selected;
        }

        private bool Blocked(vm_operations operation)
        {
            return _vm.blocked_operations != null && _vm.blocked_operations.ContainsKey(operation);
        }

        private bool Flag(string key)
        {
            return string.Equals(Value(_vm.platform, key), "true", StringComparison.OrdinalIgnoreCase);
        }

        private static string Value(Dictionary<string, string> map, string key)
        {
            if (map != null && map.TryGetValue(key, out var value))
                return value;
            return null;
        }

        private void RefreshApply()
        {
            string error = null;
            _apply.Enabled = _vm != null && !_vm.Locked && _dirty && Valid(out error);
            _error.Text = error ?? "";
            _error.Visible = !string.IsNullOrEmpty(error);
        }

        private bool Valid(out string error)
        {
            error = null;
            switch (_area)
            {
                case VmXenSettingsArea.General:
                    if (!NonNegative(_startDelay.Text))
                    {
                        error = "Start delay must be a whole number, 0 or greater.";
                        return false;
                    }
                    if (!string.IsNullOrWhiteSpace(_videoRam.Text) && !Positive(_videoRam.Text))
                    {
                        error = "Video RAM must be a whole number of MiB.";
                        return false;
                    }
                    if (!Positive(_weight.Text))
                    {
                        error = "CPU weight must be a whole number greater than 0.";
                        return false;
                    }
                    if (!NonNegative(_cap.Text))
                    {
                        error = "CPU cap must be a whole number, 0 or greater.";
                        return false;
                    }
                    return true;
                case VmXenSettingsArea.Cpu:
                    if (!Positive(_weight.Text))
                    {
                        error = "CPU weight must be a whole number greater than 0.";
                        return false;
                    }
                    if (!NonNegative(_cap.Text))
                    {
                        error = "CPU cap must be a whole number, 0 or greater.";
                        return false;
                    }
                    return true;
                case VmXenSettingsArea.SuspendSr:
                    return true;
                default:
                    throw new InvalidOperationException(_area.ToString());
            }
        }

        private static bool NonNegative(string text)
        {
            return long.TryParse(text.Trim(), out var value) && value >= 0;
        }

        private static bool Positive(string text)
        {
            return long.TryParse(text.Trim(), out var value) && value > 0;
        }

        private void ApplyClick(object sender, EventArgs e)
        {
            if (_vm == null || !Valid(out _))
                return;

            var vm = _vm;
            var snap = Read();
            string title;
            switch (_area)
            {
                case VmXenSettingsArea.General:
                    title = "Update Xen settings";
                    break;
                case VmXenSettingsArea.Cpu:
                    title = "Update CPU scheduling";
                    break;
                case VmXenSettingsArea.SuspendSr:
                    title = "Update suspend storage";
                    break;
                default:
                    throw new InvalidOperationException(_area.ToString());
            }
            var methods = Methods();
            var action = new DelegatedAsyncAction(vm.Connection, title, title, title, session => Save(session, vm, snap), methods);
            action.Completed += senderAction =>
            {
                if (!IsHandleCreated)
                    return;
                Program.Invoke(this, () =>
                {
                    if (!senderAction.Succeeded)
                    {
                        _dirty = true;
                        RefreshApply();
                        return;
                    }
                    _dirty = false;
                    LoadFromVm();
                });
            };
            _apply.Enabled = false;
            action.RunAsync();
        }

        private string[] Methods()
        {
            switch (_area)
            {
                case VmXenSettingsArea.General:
                    return new[]
                    {
                        "VM.set_other_config", "VM.set_blocked_operations", "VM.set_platform",
                        "VM.set_HVM_boot_params", "VM.set_start_delay", "VM.set_has_vendor_device",
                        "VM.set_VCPUs_params"
                    };
                case VmXenSettingsArea.Cpu:
                    return new[] { "VM.set_VCPUs_params" };
                case VmXenSettingsArea.SuspendSr:
                    return new[] { "VM.set_suspend_SR" };
                default:
                    throw new InvalidOperationException(_area.ToString());
            }
        }

        private Snapshot Read()
        {
            var snap = new Snapshot { Area = _area };
            switch (_area)
            {
                case VmXenSettingsArea.General:
                    snap.StartDelay = long.Parse(_startDelay.Text.Trim());
                    snap.Weight = _weight.Text.Trim();
                    snap.Cap = _cap.Text.Trim();
                    snap.Mask = _mask.Text.Trim();
                    snap.AutoPower = _autoPower.Checked;
                    snap.ProtectDelete = _protectDelete.Checked;
                    snap.ProtectShutdown = _protectShutdown.Checked;
                    snap.PreventMigrate = _preventMigrate.Checked;
                    snap.VendorDevice = _vendorDevice.Checked;
                    snap.Nested = _nested.Checked;
                    snap.Viridian = _viridian.Checked;
                    snap.Firmware = _firmware.SelectedItem as string ?? "bios";
                    snap.Vga = _vga.Checked;
                    snap.VideoRam = _videoRam.Text.Trim();
                    snap.SecureBoot = _secureBoot.Checked;
                    break;
                case VmXenSettingsArea.Cpu:
                    snap.Weight = _weight.Text.Trim();
                    snap.Cap = _cap.Text.Trim();
                    snap.Mask = _mask.Text.Trim();
                    break;
                case VmXenSettingsArea.SuspendSr:
                    snap.SuspendSr = (_suspendSr.SelectedItem as SrChoice)?.Sr;
                    break;
                default:
                    throw new InvalidOperationException(_area.ToString());
            }
            return snap;
        }

        private static void Save(Session session, VM vm, Snapshot snap)
        {
            var current = vm.Connection.Resolve(new XenRef<VM>(vm.opaque_ref));
            if (current == null)
                return;

            var clone = (VM)current.Clone();
            switch (snap.Area)
            {
                case VmXenSettingsArea.General:
                    ApplyGeneral(current, clone, snap);
                    break;
                case VmXenSettingsArea.Cpu:
                    clone.VCPUs_params = VM.SetDictionaryKey(current.VCPUs_params, "weight", snap.Weight);
                    clone.VCPUs_params = VM.SetDictionaryKey(clone.VCPUs_params, "cap", snap.Cap);
                    clone.VCPUs_params = VM.SetDictionaryKey(clone.VCPUs_params, "mask",
                        string.IsNullOrEmpty(snap.Mask) ? null : snap.Mask);
                    break;
                case VmXenSettingsArea.SuspendSr:
                    clone.suspend_SR = new XenRef<SR>(snap.SuspendSr == null ? Helper.NullOpaqueRef : snap.SuspendSr.opaque_ref);
                    break;
                default:
                    throw new InvalidOperationException(snap.Area.ToString());
            }

            current.Locked = true;
            try
            {
                clone.SaveChanges(session);
            }
            finally
            {
                current.Locked = false;
            }
        }

        private static void ApplyGeneral(VM current, VM clone, Snapshot snap)
        {
            clone.start_delay = snap.StartDelay;
            clone.has_vendor_device = snap.VendorDevice;
            clone.SetAutoPowerOn(snap.AutoPower);

            var blocked = current.blocked_operations == null
                ? new Dictionary<vm_operations, string>()
                : new Dictionary<vm_operations, string>(current.blocked_operations);
            SetBlocked(blocked, vm_operations.destroy, snap.ProtectDelete);
            SetBlocked(blocked, vm_operations.clean_shutdown, snap.ProtectShutdown);
            SetBlocked(blocked, vm_operations.hard_shutdown, snap.ProtectShutdown);
            SetBlocked(blocked, vm_operations.pool_migrate, snap.PreventMigrate);
            clone.blocked_operations = blocked;

            string vga = null;
            if (snap.Vga)
            {
                vga = Value(current.platform, "vga");
                if (string.IsNullOrEmpty(vga) || vga == "false")
                    vga = "std";
            }

            clone.platform = VM.SetDictionaryKeys(current.platform,
                Pair("viridian", snap.Viridian ? "true" : "false"),
                Pair("nested-virt", snap.Nested ? "true" : "false"),
                Pair("exp-nested-hvm", snap.Nested ? "true" : "false"),
                Pair("vga", vga),
                Pair("videoram", string.IsNullOrEmpty(snap.VideoRam) ? null : snap.VideoRam),
                Pair("secureboot", snap.SecureBoot ? "true" : "false"));

            if (current.IsHVM())
                clone.HVM_boot_params = VM.SetDictionaryKey(current.HVM_boot_params, "firmware", snap.Firmware);

            clone.VCPUs_params = VM.SetDictionaryKey(current.VCPUs_params, "weight", snap.Weight);
            clone.VCPUs_params = VM.SetDictionaryKey(clone.VCPUs_params, "cap", snap.Cap);
            clone.VCPUs_params = VM.SetDictionaryKey(clone.VCPUs_params, "mask",
                string.IsNullOrEmpty(snap.Mask) ? null : snap.Mask);
        }

        private static void SetBlocked(Dictionary<vm_operations, string> blocked, vm_operations operation, bool block)
        {
            if (block)
                blocked[operation] = "true";
            else
                blocked.Remove(operation);
        }

        private static KeyValuePair<string, string> Pair(string key, string value)
        {
            return new KeyValuePair<string, string>(key, value);
        }

        private sealed class Snapshot
        {
            public VmXenSettingsArea Area;
            public long StartDelay;
            public bool AutoPower;
            public bool ProtectDelete;
            public bool ProtectShutdown;
            public bool PreventMigrate;
            public bool VendorDevice;
            public bool Nested;
            public bool Viridian;
            public string Firmware;
            public bool Vga;
            public string VideoRam;
            public bool SecureBoot;
            public string Weight;
            public string Cap;
            public string Mask;
            public SR SuspendSr;
        }

        private sealed class SrChoice
        {
            public SrChoice(SR sr)
            {
                Sr = sr;
            }

            public SR Sr { get; }

            public override string ToString()
            {
                return Sr == null ? "None" : Sr.Name();
            }
        }
    }
}
