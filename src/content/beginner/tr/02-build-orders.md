---
slug: 02-build-orders
title: "Yapım Sıraları"
description: "AoE2 yapım sıraları: build order nedir, adımlar nasıl okunur ve üç temel açılış — 18-Pop Scouts, 19-Pop Archers ve Fast Castle."
order: 2
prereq: [01-resources]
---

Bir **yapım sırası** (build order), Karanlık Çağ ve Feudal geçişi boyunca her köylüyü tam olarak nereye koyacağınızı söyleyen yapılandırılmış bir dizidir. Birini takip etmek tahmin yürütmeyi ortadan kaldırır ve keşfe, duvar örmeye ve rakibinize tepki vermeye odaklanmanızı sağlar.

## Neden yapım sırası kullanmalı

Yapım sırası olmadan oyuncular köylüleri rastgele dağıtır ve sonunda şöyle olur:
- Tarla gerektiğinde odunsuz kalırlar
- Kale Çağı'nı araştırmak gerektiğinde altınsız kalırlar
- Yemek 50'nin altına düştüğü için Şehir Merkezi boşta kalır

Bir yapım sırası bunların hepsini ortadan kaldırır. Sabah kalkar, 1–25. adımları takip eder ve her oyunda aynı dakikada, aynı kaynaklar hazır şekilde Feudal Çağ'a ulaşırsınız.

## Yapım sıraları nasıl yazılır

Her adım şunları listeler:
- **Nüfus sayısı** (örn. "Köylü 12")
- **Zaman**, m:ss biçiminde (örn. "2:35")
- **Eylem** ("→ ev, sonra çalılara değirmen yap")

Örneğin 18-Pop 1-Ahır Scout Rush şöyle başlar:

```
0:00   6 köylü → ŞM altındaki koyunlar
0:50   +2 köylü → odun, oduncu kampı yap
1:15   +1 köylü → en yakın domuzu çek
1:40   +1 köylü → ev, sonra çalılara değirmen yap
```

## Üç temel açılış

Her oyuncu şu üçünü öğrenmeli:

1. **18-Pop Scouts** — hızlı süvariyle Feudal Çağ baskısı. Açık haritalarda "varsayılan" açılış.
2. **19-Pop Archers** — menzilli Feudal baskısı. Okçu uygarlıkları için "varsayılan".
3. **Fast Castle into Boom** — Feudal saldırganlığını atla, Kale Çağı'na ve ek Şehir Merkezlerine koş. Kapalı haritalarda "varsayılan".

Bu üçünü içselleştirdikten sonra diğer her yapım bir varyasyondur: drush daha erken bir kışla ekler, MAA Feudal'da militiayı yükseltir, Tower Rush rakip üssüne 10 köylü gönderir vb.

## Pratik ipucu

Gerçek bir oyunda kullanmadan önce yapım sırasını üç kez tek başınıza (Kolay AI'ya karşı) çalıştırın. Kendinizi kaydedin ya da sadece saate bakın — Feudal Çağ tıklama zamanınız 30 saniyeden fazla saparsa baştan başlayın ve hangi adımı kaçırdığınızı belirleyin.

## Sonraki bölüm

Yakında: **Uygarlıklar** — bir uygarlık nasıl seçilir ve özgün bonusları oyun planınız için ne anlama gelir.
